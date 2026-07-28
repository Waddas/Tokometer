import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LimitWindow, UsageSnapshot } from "./api";
import { UsageRenderer } from "./usage";

// The renderer reads Date.now() for reset countdowns; pin the clock.
const NOW_MS = 1_700_000_000_000;

/** A reset `mins` minutes into the future, as epoch seconds. */
function resetInMinutes(mins: number): number {
  return Math.round((NOW_MS + mins * 60_000) / 1000);
}

function window_(
  id: string,
  label: string,
  utilization: number,
  resetAt: number | null = null,
): LimitWindow {
  return { id, label, utilization, resetAt };
}

/** A window carried over from an earlier poll (carry_missing_windows). */
function staleWindow(
  id: string,
  label: string,
  utilization: number,
  resetAt: number | null = null,
): LimitWindow {
  return { ...window_(id, label, utilization, resetAt), stale: true };
}

function snapshot(windows: LimitWindow[]): UsageSnapshot {
  return {
    status: "ok",
    source: "oauth",
    fetchedAt: NOW_MS,
    windows,
    error: null,
  };
}

describe("UsageRenderer", () => {
  let renderer: UsageRenderer;
  let container: HTMLElement;
  let tileCounts: number[];

  const tile = (id: string) => document.querySelector(`[data-window="${id}"]`) as HTMLElement;
  const part = (id: string, cls: string) =>
    document.querySelector(`[data-window="${id}"] .${cls}`) as HTMLElement;
  const sessionPct = () => part("session", "pct");
  const sessionReset = () => part("session", "reset-value");
  const labels = () =>
    [...document.querySelectorAll(".panel .label")].map((el) => el.textContent);
  // The label also prefixes the countdown; styles.css shows one form or the
  // other depending on the tile's height.
  const resetLabels = () =>
    [...document.querySelectorAll(".panel .reset-label")].map((el) => el.textContent);

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
    localStorage.clear(); // the info view's tile ids are read at construction
    document.body.innerHTML = `<div id="content"></div>`;
    container = document.getElementById("content")!;
    tileCounts = [];
    renderer = new UsageRenderer(container, (n) => tileCounts.push(n));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("tiles", () => {
    it("shows two placeholder tiles before any data arrives", () => {
      expect(labels()).toEqual(["5h", "7d"]);
      expect(sessionPct().textContent).toBe("--%");
      expect(sessionReset().textContent).toBe("---");
      expect(tileCounts).toEqual([2]);
    });

    it("renders one labelled tile per window, in the snapshot's order", () => {
      renderer.update(
        snapshot([
          window_("session", "5h", 19),
          window_("weekly_all", "7d", 20),
          window_("weekly_scoped:fable", "Fable", 21),
        ]),
      );
      expect(labels()).toEqual(["5h", "7d", "Fable"]);
      expect(part("weekly_scoped:fable", "pct").textContent).toBe("21%");
      expect(tileCounts).toEqual([2, 3]);
    });

    it("also renders each label as the countdown line's prefix", () => {
      renderer.update(
        snapshot([
          window_("session", "5h", 19, resetInMinutes(188)),
          window_("weekly_scoped:fable", "Fable", 21),
        ]),
      );
      expect(resetLabels()).toEqual(["5h | ", "Fable | "]);
      expect(part("session", "reset").textContent).toBe("5h | 3h 8m");
      expect(part("weekly_scoped:fable", "reset").textContent).toBe("Fable | ---");
    });

    it("adds and drops tiles as the reported windows change", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      expect(labels()).toEqual(["5h"]);
      renderer.update(snapshot([window_("session", "5h", 19), window_("weekly_all", "7d", 20)]));
      expect(labels()).toEqual(["5h", "7d"]);
      expect(tileCounts).toEqual([2, 1, 2]);
    });

    it("keeps the tile elements across polls that report the same windows", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      const before = tile("session");
      renderer.update(snapshot([window_("session", "5h", 25)]));
      expect(tile("session")).toBe(before);
      expect(sessionPct().textContent).toBe("25%");
    });

    it("falls back to the placeholders when a poll carries no windows", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      renderer.update({ ...snapshot([]), status: "error", error: "boom" });
      expect(labels()).toEqual(["5h", "7d"]);
      expect(sessionPct().textContent).toBe("--%");
    });
  });

  describe("hidden limits", () => {
    it("leaves hidden windows out and reports the smaller tile count", () => {
      renderer.update(
        snapshot([
          window_("session", "5h", 19),
          window_("weekly_all", "7d", 20),
          window_("weekly_scoped:fable", "Fable", 21),
        ]),
      );
      renderer.setHidden(["weekly_scoped:fable"]);
      expect(labels()).toEqual(["5h", "7d"]);
      expect(tileCounts[tileCounts.length - 1]).toBe(2);
    });

    it("still reports one tile of room when every limit is hidden", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      renderer.setHidden(["session"]);
      expect(labels()).toEqual([]);
      expect(tileCounts[tileCounts.length - 1]).toBe(1);
    });

    it("shows a limit again once it is unhidden", () => {
      renderer.setHidden(["weekly_all"]);
      renderer.update(snapshot([window_("session", "5h", 19), window_("weekly_all", "7d", 20)]));
      expect(labels()).toEqual(["5h"]);
      renderer.setHidden([]);
      expect(labels()).toEqual(["5h", "7d"]);
    });
  });

  describe("percentage rendering", () => {
    it("rounds utilization and shows it with a percent sign", () => {
      renderer.update(snapshot([window_("session", "5h", 42.6)]));
      expect(sessionPct().textContent).toBe("43%");
    });

    it("colours below 50% green", () => {
      renderer.update(snapshot([window_("session", "5h", 49.4)]));
      expect(sessionPct().style.color).toBe("var(--green)");
    });

    it("colours 50–79% amber", () => {
      renderer.update(snapshot([window_("session", "5h", 50)]));
      expect(sessionPct().style.color).toBe("var(--amber)");
    });

    it("colours 80%+ red", () => {
      renderer.update(snapshot([window_("session", "5h", 80)]));
      expect(sessionPct().style.color).toBe("var(--red)");
    });

    it("drains the colours to grey while the data is stale", () => {
      renderer.update(snapshot([window_("session", "5h", 80)]), true);
      expect(sessionPct().style.color).toBe("var(--dim)");
    });

    it("greys a carried-over window even when the snapshot is fresh", () => {
      renderer.update(
        snapshot([window_("session", "5h", 80), staleWindow("weekly_scoped:fable", "Fable", 21)]),
      );
      // The probe's own reading stays coloured; the carried one is dimmed.
      expect(sessionPct().style.color).toBe("var(--red)");
      expect(part("weekly_scoped:fable", "pct").style.color).toBe("var(--dim)");
    });

    it("updates each window's tile independently", () => {
      renderer.update(snapshot([window_("session", "5h", 10), window_("weekly_all", "7d", 90)]));
      expect(sessionPct().textContent).toBe("10%");
      expect(sessionPct().style.color).toBe("var(--green)");
      expect(part("weekly_all", "pct").textContent).toBe("90%");
      expect(part("weekly_all", "pct").style.color).toBe("var(--red)");
    });
  });

  describe("reset countdown formatting", () => {
    const withReset = (mins: number) =>
      renderer.update(snapshot([window_("session", "5h", 10, resetInMinutes(mins))]));

    it("shows minutes under an hour", () => {
      withReset(45);
      expect(sessionReset().textContent).toBe("45m");
    });

    it("shows hours and minutes between 1h and 1d", () => {
      withReset(150);
      expect(sessionReset().textContent).toBe("2h 30m");
    });

    it("shows days and hours beyond 24h", () => {
      withReset(2 * 1440 + 180);
      expect(sessionReset().textContent).toBe("2d 3h");
    });

    it("clamps an elapsed reset to 0m rather than going negative", () => {
      withReset(-30);
      expect(sessionReset().textContent).toBe("0m");
    });

    it("shows --- when there is no reset time", () => {
      renderer.update(snapshot([window_("session", "5h", 10)]));
      expect(sessionReset().textContent).toBe("---");
    });
  });

  // Compact tiles show the percentage alone (a container query in styles.css,
  // so not exercisable here); the label and countdown live in the tooltip and
  // in the info view a right-click flips the tile to.
  describe("tile tooltips", () => {
    it("gives every tile a tooltip naming the limit and its countdown", () => {
      renderer.update(
        snapshot([
          window_("session", "5h", 19, resetInMinutes(85)),
          window_("weekly_scoped:fable", "Fable", 21, resetInMinutes(2 * 1440 + 900)),
        ]),
      );
      expect(tile("session").title).toBe("5h — resets in 1h 25m");
      expect(tile("weekly_scoped:fable").title).toBe("Fable — resets in 2d 15h");
    });

    it("names the limit alone when there is no reset time", () => {
      renderer.update(snapshot([window_("weekly_scoped:fable", "Fable", 21)]));
      expect(tile("weekly_scoped:fable").title).toBe("Fable");
    });

    it("refreshes the tooltips on the 30s interval", () => {
      renderer.update(snapshot([window_("session", "5h", 10, resetInMinutes(45))]));
      expect(tile("session").title).toBe("5h — resets in 45m");
      vi.advanceTimersByTime(5 * 60_000);
      expect(tile("session").title).toBe("5h — resets in 40m");
    });

    it("marks a carried-over window's tooltip as a last known value", () => {
      renderer.update(
        snapshot([
          window_("session", "5h", 19, resetInMinutes(85)),
          staleWindow("weekly_scoped:fable", "Fable", 21, resetInMinutes(2 * 1440 + 900)),
        ]),
      );
      expect(tile("session").title).toBe("5h — resets in 1h 25m");
      expect(tile("weekly_scoped:fable").title).toBe("Fable — resets in 2d 15h (last known)");
    });
  });

  // Right-click flips a tile to the info view: the limit's name over its
  // countdown, no percentage (the layout itself is CSS, see styles.css).
  describe("info view", () => {
    const rightClick = (id: string) =>
      tile(id).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

    it("flips one tile's view on right-click and back again", () => {
      renderer.update(snapshot([window_("session", "5h", 19), window_("weekly_all", "7d", 20)]));
      rightClick("session");
      expect(tile("session").classList.contains("info")).toBe(true);
      expect(tile("weekly_all").classList.contains("info")).toBe(false);
      rightClick("session");
      expect(tile("session").classList.contains("info")).toBe(false);
    });

    it("persists the flipped tiles", () => {
      renderer.update(snapshot([window_("session", "5h", 19), window_("weekly_all", "7d", 20)]));
      rightClick("weekly_all");
      expect(localStorage.getItem("tile-info-view")).toBe("weekly_all");
      rightClick("session");
      expect(localStorage.getItem("tile-info-view")).toBe("weekly_all,session");
      rightClick("weekly_all");
      expect(localStorage.getItem("tile-info-view")).toBe("session");
    });

    it("restores the flipped tiles on a fresh renderer", () => {
      localStorage.setItem("tile-info-view", "weekly_scoped:fable");
      document.body.innerHTML = `<div id="content"></div>`;
      const fresh = new UsageRenderer(document.getElementById("content")!, () => {});
      fresh.update(
        snapshot([window_("session", "5h", 19), window_("weekly_scoped:fable", "Fable", 21)]),
      );
      expect(tile("weekly_scoped:fable").classList.contains("info")).toBe(true);
      expect(tile("session").classList.contains("info")).toBe(false);
    });

    it("re-applies the view when the reported windows change the tiles", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      rightClick("session");
      renderer.update(snapshot([window_("session", "5h", 19), window_("weekly_all", "7d", 20)]));
      expect(tile("session").classList.contains("info")).toBe(true);
    });

    it("keeps refreshing the countdown while the tile shows it", () => {
      renderer.update(snapshot([window_("session", "5h", 10, resetInMinutes(45))]));
      rightClick("session");
      expect(sessionReset().textContent).toBe("45m");
      vi.advanceTimersByTime(5 * 60_000);
      expect(sessionReset().textContent).toBe("40m");
    });

    it("keeps the right-click from reaching the window's own handler", () => {
      renderer.update(snapshot([window_("session", "5h", 19)]));
      const onWindow = vi.fn();
      window.addEventListener("contextmenu", onWindow);
      const dispatched = rightClick("session");
      window.removeEventListener("contextmenu", onWindow);
      expect(onWindow).not.toHaveBeenCalled();
      expect(dispatched).toBe(false); // default prevented
    });
  });

  describe("periodic reset refresh", () => {
    it("recomputes countdowns on the 30s interval without a new snapshot", () => {
      renderer.update(snapshot([window_("session", "5h", 10, resetInMinutes(45))]));
      expect(sessionReset().textContent).toBe("45m");

      // 5 real minutes pass; the interval should redraw a smaller countdown.
      vi.advanceTimersByTime(5 * 60_000);
      expect(sessionReset().textContent).toBe("40m");
    });

    it("does nothing on the interval before any snapshot arrives", () => {
      vi.advanceTimersByTime(60_000);
      expect(sessionReset().textContent).toBe("---");
    });
  });
});
