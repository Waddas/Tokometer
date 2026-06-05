import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { UsageSnapshot, UsageWindow } from "./api";
import { UsageRenderer } from "./usage";

// The renderer reads Date.now() for reset countdowns; pin the clock.
const NOW_MS = 1_700_000_000_000;

function panelHtml(name: string): string {
  return `<section class="panel" data-window="${name}">
    <div class="pct">--%</div>
    <div class="reset">---</div>
  </section>`;
}

/** A reset `mins` minutes into the future, as epoch seconds. */
function resetInMinutes(mins: number): number {
  return Math.round((NOW_MS + mins * 60_000) / 1000);
}

function snapshot(
  fiveHour: UsageWindow | null,
  sevenDay: UsageWindow | null,
): UsageSnapshot {
  return {
    status: "ok",
    source: "oauth",
    fetchedAt: NOW_MS,
    fiveHour,
    sevenDay,
    fiveHourStatus: null,
    error: null,
  };
}

describe("UsageRenderer", () => {
  let renderer: UsageRenderer;

  const sessionPct = () => document.querySelector('[data-window="session"] .pct') as HTMLElement;
  const sessionReset = () => document.querySelector('[data-window="session"] .reset') as HTMLElement;
  const weeklyPct = () => document.querySelector('[data-window="weekly"] .pct') as HTMLElement;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
    document.body.innerHTML = panelHtml("session") + panelHtml("weekly");
    renderer = new UsageRenderer(document.body);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("percentage rendering", () => {
    it("rounds utilization and shows it with a percent sign", () => {
      renderer.update(snapshot({ utilization: 42.6, resetAt: null }, null));
      expect(sessionPct().textContent).toBe("43%");
    });

    it("colours below 50% green", () => {
      renderer.update(snapshot({ utilization: 49.4, resetAt: null }, null));
      expect(sessionPct().style.color).toBe("var(--green)");
    });

    it("colours 50–79% amber", () => {
      renderer.update(snapshot({ utilization: 50, resetAt: null }, null));
      expect(sessionPct().style.color).toBe("var(--amber)");
    });

    it("colours 80%+ red", () => {
      renderer.update(snapshot({ utilization: 80, resetAt: null }, null));
      expect(sessionPct().style.color).toBe("var(--red)");
    });

    it("renders a null window as a dimmed placeholder", () => {
      renderer.update(snapshot(null, null));
      expect(sessionPct().textContent).toBe("--%");
      expect(sessionPct().style.color).toBe("var(--dim)");
      expect(sessionReset().textContent).toBe("---");
    });

    it("updates the session and weekly panels independently", () => {
      renderer.update(
        snapshot(
          { utilization: 10, resetAt: null },
          { utilization: 90, resetAt: null },
        ),
      );
      expect(sessionPct().textContent).toBe("10%");
      expect(sessionPct().style.color).toBe("var(--green)");
      expect(weeklyPct().textContent).toBe("90%");
      expect(weeklyPct().style.color).toBe("var(--red)");
    });
  });

  describe("reset countdown formatting", () => {
    it("shows minutes under an hour", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: resetInMinutes(45) }, null));
      expect(sessionReset().textContent).toBe("45m");
    });

    it("shows hours and minutes between 1h and 1d", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: resetInMinutes(150) }, null));
      expect(sessionReset().textContent).toBe("2h 30m");
    });

    it("shows days and hours beyond 24h", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: resetInMinutes(2 * 1440 + 180) }, null));
      expect(sessionReset().textContent).toBe("2d 3h");
    });

    it("clamps an elapsed reset to 0m rather than going negative", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: resetInMinutes(-30) }, null));
      expect(sessionReset().textContent).toBe("0m");
    });

    it("shows --- when there is no reset time", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: null }, null));
      expect(sessionReset().textContent).toBe("---");
    });
  });

  describe("periodic reset refresh", () => {
    it("recomputes countdowns on the 30s interval without a new snapshot", () => {
      renderer.update(snapshot({ utilization: 10, resetAt: resetInMinutes(45) }, null));
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
