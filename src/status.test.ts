import { describe, expect, it } from "vitest";
import type { UsageSnapshot } from "./api";
import { friendlyError, usageStatus } from "./status";

const now = 1_800_000_000_000;
const failed: UsageSnapshot = {
  provider: "codex", scope: "codex:a", status: "error", source: null,
  fetchedAt: now, windows: [], error: "Cannot reach Codex usage — check your connection",
  lastSuccessAt: now - 120_000,
};

describe("usage status", () => {
  it.each([
    ["Codex login expired — open Codex", "Open Codex to refresh your login"],
    ["Codex usage access denied for this account", "Codex usage access denied"],
    ["Codex usage rate-limited — waiting before retrying", "Codex rate limit — waiting"],
    ["Codex usage request timed out — retrying", "Codex request timed out — retrying"],
    ["Codex usage: HTTP 503", "Codex service error (503) — retrying"],
    ["Codex usage: HTTP 404", "Codex usage error (HTTP 404)"],
    ["Codex usage returned an unreadable response", "Codex sent an invalid response"],
    ["no Claude credentials", "Sign in to Claude Code to start tracking"],
  ])("distinguishes %s", (error, expected) => {
    expect(friendlyError(error)).toBe(expected);
  });

  it("keeps the age visible during grace, without making the reading appear fresh", () => {
    expect(usageStatus({ ...failed, recovering: true }, now)).toBe("2m old · Updating…");
    expect(usageStatus({ ...failed, recovering: true }, now + 60_000)).toBe("3m old · Updating…");
    expect(usageStatus({ ...failed, recovering: true }, now + 180_000))
      .toBe("5m old · Can't reach usage API — retrying");
  });

  it("shows rate-limit cooldowns and stops counting once due", () => {
    const limited = { ...failed, error: "Codex usage rate-limited", retryAt: now + 90_000 };
    expect(usageStatus(limited, now)).toBe("2m old · Codex rate limit — waiting · retry in 2m");
    expect(usageStatus(limited, now + 90_000)).not.toContain("retry in");
  });

  it("does not invent an age for legacy errors and clears status after recovery", () => {
    expect(usageStatus({ ...failed, lastSuccessAt: undefined, recovering: true }, now))
      .toBe("Can't reach usage API — retrying");
    expect(usageStatus({ ...failed, status: "ok" }, now)).toBe("");
  });
});
