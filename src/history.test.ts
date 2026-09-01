import { describe, expect, it } from "vitest";
import { UsageHistory } from "./history";
import { SESSION_ID, WEEKLY_ALL_ID, type UsageSnapshot } from "./api";

function snapshot(
  fetchedAt: number,
  five: number | null,
  week: number | null,
  /** epoch seconds, as the API reports it */
  fiveReset: number | null = null,
): UsageSnapshot {
  const windows = [];
  if (five !== null) {
    windows.push({ id: SESSION_ID, label: "5h", utilization: five, resetAt: fiveReset });
  }
  if (week !== null) {
    windows.push({ id: WEEKLY_ALL_ID, label: "7d", utilization: week, resetAt: null });
  }
  return { status: "ok", source: "oauth", fetchedAt, windows, error: null };
}

const MIN = 60_000;

describe("UsageHistory", () => {
  it("records samples and serves window points", () => {
    const h = new UsageHistory();
    h.sample(snapshot(0, 10, 5), 0);
    h.sample(snapshot(MIN, 12, null), MIN);
    expect(h.points(SESSION_ID, 0, null)).toEqual([
      { ms: 0, pct: 10 },
      { ms: MIN, pct: 12 },
    ]);
    // Windows the poll lacked are skipped per id, not dropped entirely.
    expect(h.points(WEEKLY_ALL_ID, 0, null)).toEqual([{ ms: 0, pct: 5 }]);
  });

  it("records every window in the snapshot, scoped ones included", () => {
    const h = new UsageHistory();
    const s = snapshot(0, 10, null);
    s.windows.push({ id: "weekly_scoped:fable", label: "Fable", utilization: 21, resetAt: 18_000 });
    h.sample(s, 0);
    expect(h.points("weekly_scoped:fable", 0, 18_000_000)).toEqual([{ ms: 0, pct: 21 }]);
  });

  it("serves nothing for a window the log has never seen", () => {
    const h = new UsageHistory();
    h.sample(snapshot(0, 10, null), 0);
    expect(h.points("monthly_all", 0, null)).toEqual([]);
  });

  it("filters points by window start", () => {
    const h = new UsageHistory();
    h.sample(snapshot(0, 10, null), 0);
    h.sample(snapshot(10 * MIN, 20, null), 10 * MIN);
    expect(h.points(SESSION_ID, 5 * MIN, null)).toEqual([{ ms: 10 * MIN, pct: 20 }]);
  });

  it("ignores error snapshots and near-duplicate fetches", () => {
    const h = new UsageHistory();
    h.sample(snapshot(0, 10, null), 0);
    h.sample(snapshot(0, 10, null), 5_000); // startup replay of the same poll
    h.sample({ ...snapshot(MIN, 50, null), status: "error" }, MIN);
    expect(h.points(SESSION_ID, 0, null)).toHaveLength(1);
  });

  it("segments points by the current reset time, dropping other windows'", () => {
    const h = new UsageHistory();
    const HOUR = 60 * MIN;
    h.sample(snapshot(1 * HOUR, 80, null, (6 * HOUR) / 1000), 1 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, (13 * HOUR) / 1000), 8 * HOUR);
    expect(h.points(SESSION_ID, 0, 13 * HOUR)).toEqual([{ ms: 8 * HOUR, pct: 5 }]);
    // Unstamped legacy samples survive on the time filter alone.
    h.sample(snapshot(9 * HOUR, 12, null), 9 * HOUR);
    expect(h.points(SESSION_ID, 0, 13 * HOUR)).toHaveLength(2);
  });

  it("keeps only unstamped samples while no window is running", () => {
    const h = new UsageHistory();
    const HOUR = 60 * MIN;
    h.sample(snapshot(1 * HOUR, 80, null, (6 * HOUR) / 1000), 1 * HOUR);
    h.sample(snapshot(7 * HOUR, 0, null), 7 * HOUR); // polled during the lapse
    expect(h.points(SESSION_ID, 0, null)).toEqual([{ ms: 7 * HOUR, pct: 0 }]);
  });

  it("serves the backend log after load, replacing prior samples", () => {
    const h = new UsageHistory();
    h.sample(snapshot(50 * MIN, 99, null), 50 * MIN);
    h.load([
      { ms: 0, w: { [SESSION_ID]: { pct: 10, reset: null } } },
      {
        ms: MIN,
        w: {
          [SESSION_ID]: { pct: 12, reset: 5 * 60 * MIN * 1000 },
          [WEEKLY_ALL_ID]: { pct: 3, reset: null },
        },
      },
    ]);
    expect(h.points(SESSION_ID, 0, 5 * 60 * MIN * 1000)).toEqual([
      { ms: 0, pct: 10 },
      { ms: MIN, pct: 12 },
    ]);
  });
});

describe("previousWindow", () => {
  const HOUR = 60 * MIN;
  const WINDOW = 5 * HOUR;
  /** ms → the epoch-seconds value the API would report. */
  const sec = (ms: number) => ms / 1000;

  // A busy window resetting at 6h, an idle gap, then the current window
  // (first message at 8h, so it resets at 13h).
  function twoWindows() {
    const h = new UsageHistory();
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null, sec(6 * HOUR)), 3 * HOUR);
    h.sample(snapshot(5 * HOUR, 80, null, sec(6 * HOUR)), 5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    h.sample(snapshot(9 * HOUR, 12, null, sec(13 * HOUR)), 9 * HOUR);
    return h;
  }

  it("segments the previous window by its polled reset time, not wall-clock arithmetic", () => {
    const prev = twoWindows().previousWindow(SESSION_ID, 13 * HOUR, WINDOW);
    expect(prev).not.toBeNull();
    expect(prev!.resetMs).toBe(6 * HOUR);
    expect(prev!.pts).toEqual([
      { ms: 1 * HOUR, pct: 10 },
      { ms: 3 * HOUR, pct: 40 },
      { ms: 5 * HOUR, pct: 80 },
    ]);
  });

  it("excludes the current window's own samples", () => {
    const prev = twoWindows().previousWindow(SESSION_ID, 13 * HOUR, WINDOW)!;
    expect(prev.pts.every((p) => p.ms <= 6 * HOUR)).toBe(true);
  });

  it("treats jittered reset times from mixed sources as one window", () => {
    const h = new UsageHistory();
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(2 * HOUR, 30, null, sec(6 * HOUR) + 45), 2 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    const prev = h.previousWindow(SESSION_ID, 13 * HOUR, WINDOW)!;
    expect(prev.pts).toHaveLength(2);
  });

  it("serves the latest completed window when no window is running", () => {
    const h = new UsageHistory();
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(5 * HOUR, 80, null, sec(6 * HOUR)), 5 * HOUR);
    const prev = h.previousWindow(SESSION_ID, null, WINDOW)!;
    expect(prev.resetMs).toBe(6 * HOUR);
    expect(prev.pts).toHaveLength(2);
  });

  it("returns null when the previous window has fewer than two samples", () => {
    const h = new UsageHistory();
    h.sample(snapshot(5 * HOUR, 2, null, sec(6 * HOUR)), 5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    expect(h.previousWindow(SESSION_ID, 13 * HOUR, WINDOW)).toBeNull();
  });

  it("returns null for samples from older builds that carry no reset time", () => {
    const h = new UsageHistory();
    h.sample(snapshot(1 * HOUR, 10, null), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null), 3 * HOUR);
    expect(h.previousWindow(SESSION_ID, 13 * HOUR, WINDOW)).toBeNull();
  });

  it("keeps only samples inside the previous window's actual span", () => {
    const h = new UsageHistory();
    // A sample stamped with the old reset but taken after it (stale poll).
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null, sec(6 * HOUR)), 3 * HOUR);
    h.sample(snapshot(6.5 * HOUR, 40, null, sec(6 * HOUR)), 6.5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    const prev = h.previousWindow(SESSION_ID, 13 * HOUR, WINDOW)!;
    expect(prev.pts.map((p) => p.ms)).toEqual([1 * HOUR, 3 * HOUR]);
  });
});

describe("rateProfile", () => {
  const HOUR = 60 * MIN;
  const sec = (ms: number) => ms / 1000;
  // Monday 10:00 local, so every poll below lands in one hour-of-week bucket
  // and the profile's rate there is just total gain over total time.
  const t0 = new Date(2026, 0, 5, 10).getTime();

  it("learns from gains within a window, restarts at a rollover, ignores a lapse", () => {
    const h = new UsageHistory();
    h.sample(snapshot(t0, 10, null, sec(t0 + HOUR)), t0);
    h.sample(snapshot(t0 + 30 * MIN, 30, null, sec(t0 + HOUR)), t0 + 30 * MIN); // +20
    h.sample(snapshot(t0 + 45 * MIN, 5, null, sec(t0 + 6 * HOUR)), t0 + 45 * MIN); // new window: +5
    h.sample(snapshot(t0 + 60 * MIN, 0, null, null), t0 + 60 * MIN); // lapsed: nothing
    expect(h.rateProfile(SESSION_ID).rateAt(t0) * HOUR).toBeCloseTo(25);
  });

  it("rebuilds after new samples arrive", () => {
    const h = new UsageHistory();
    h.sample(snapshot(t0, 10, null, sec(t0 + HOUR)), t0);
    h.sample(snapshot(t0 + 30 * MIN, 20, null, sec(t0 + HOUR)), t0 + 30 * MIN);
    const before = h.rateProfile(SESSION_ID).rateAt(t0);
    h.sample(snapshot(t0 + 45 * MIN, 50, null, sec(t0 + HOUR)), t0 + 45 * MIN);
    expect(h.rateProfile(SESSION_ID).rateAt(t0)).toBeGreaterThan(before);
  });

  it("is empty for a window the log never saw", () => {
    const h = new UsageHistory();
    h.sample(snapshot(t0, 10, null, sec(t0 + HOUR)), t0);
    expect(h.rateProfile("weekly_scoped:fable").hasData).toBe(false);
  });
});
