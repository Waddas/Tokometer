import { expect, it, vi } from "vitest";
import settingsHtml from "../settings.html?raw";
import type { AppStateSnapshot, StateChange, UsageSnapshot } from "./api";

const callbacks = vi.hoisted(() => ({
  state: (_: StateChange) => {},
  usage: (_: UsageSnapshot) => {},
  setProvider: vi.fn(),
  setControlSides: vi.fn(),
}));
const initial: AppStateSnapshot = {
  controlsSide: "top", providersSide: "right",
  provider: "claude", pin: false, layout: "mascot-left", size: "small", customScale: null,
  mascot: "clawd", trayStyle: "ring", theme: "paper", workDays: Array(7).fill(true),
  probeFallback: true, hiddenLimits: [], beta: {}, lastUsage: null,
};

vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.5.0" }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ show: async () => {}, setFocus: async () => {} }) }));
vi.mock("./api", async (importOriginal) => ({
  ...await importOriginal<typeof import("./api")>(),
  getState: async () => initial,
  getAutostart: async () => false,
  setProvider: callbacks.setProvider,
  setControlSides: callbacks.setControlSides,
  onStateChange: async (cb: typeof callbacks.state) => { callbacks.state = cb; },
  onUsage: async (cb: typeof callbacks.usage) => { callbacks.usage = cb; },
  onUpdatePhase: async () => {},
  onUpdateProgress: async () => {},
}));

it("switches providers through IPC, hides the Claude probe, and rejects late Claude usage", async () => {
  const page = new DOMParser().parseFromString(settingsHtml, "text/html");
  page.querySelectorAll("script").forEach((script) => script.remove());
  document.body.innerHTML = page.body.innerHTML;
  await import("./settings");
  await vi.waitFor(() => expect(document.querySelector('#opt-provider button[aria-pressed="true"]')?.textContent).toBe("Claude"));
  document.querySelectorAll<HTMLButtonElement>("#opt-controls-side button")[2].click();
  expect(callbacks.setControlSides).toHaveBeenLastCalledWith("right", "right");
  document.querySelectorAll<HTMLButtonElement>("#opt-providers-side button")[3].click();
  expect(callbacks.setControlSides).toHaveBeenLastCalledWith("right", "bottom");
  const codex = document.querySelectorAll<HTMLButtonElement>("#opt-provider button")[1];
  codex.click();
  expect(codex.disabled).toBe(true);
  expect(callbacks.setProvider).not.toHaveBeenCalled();
  callbacks.state({ ...initial, availableProviders: { claude: true, codex: true }, visible: true });
  expect(codex.disabled).toBe(false);
  codex.click();
  expect(callbacks.setProvider).toHaveBeenCalledWith("codex");
  callbacks.state({ ...initial, provider: "codex", visible: true });
  expect(document.getElementById("probe-options")!.hidden).toBe(true);
  expect(codex.getAttribute("aria-pressed")).toBe("true");
  callbacks.usage({ provider: "claude", status: "ok", source: "oauth", fetchedAt: 1,
    windows: [{ id: "weekly_scoped:fable", label: "Fable", utilization: 10, resetAt: null }], error: null });
  expect(document.getElementById("limits")!.textContent).not.toContain("Fable");
  callbacks.usage({ provider: "codex", scope: "codex:fixture", status: "error", source: null,
    fetchedAt: 2, windows: [], error: "Codex login expired — open Codex to refresh your login, then retry" });
  expect(document.getElementById("provider-status")!.textContent).toContain("Codex login expired");
  callbacks.state({ ...initial, visible: true });
  expect(document.getElementById("probe-options")!.hidden).toBe(false);
  expect(document.getElementById("provider-status")!.hidden).toBe(true);
  expect(document.documentElement.dataset.theme).toBe("paper");
});
