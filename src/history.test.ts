import { describe, expect, it } from "vitest";
import { UsageHistory } from "./history";
import type { UsageSnapshot } from "./api";

function snapshot(
  fetchedAt: number,
  five: number | null,
  week: number | null,
  /** epoch seconds, as the API reports it */
  fiveReset: number | null = null,
): UsageSnapshot {
  return {
    status: "ok",
    source: "oauth",
    fetchedAt,
    fiveHour: five === null ? null : { utilization: five, resetAt: fiveReset },
    sevenDay: week === null ? null : { utilization: week, resetAt: null },
    fiveHourStatus: null,
    error: null,
  };
}

function fakeStore(initial: Record<string, string> = {}) {
  const data = { ...initial };
  return {
    data,
    getItem: (k: string) => data[k] ?? null,
    setItem: (k: string, v: string) => {
      data[k] = v;
    },
  };
}

const MIN = 60_000;

describe("UsageHistory", () => {
  it("records samples and serves window points", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(0, 10, 5), 0);
    h.sample(snapshot(MIN, 12, null), MIN);
    expect(h.points("five", 0)).toEqual([
      { ms: 0, pct: 10 },
      { ms: MIN, pct: 12 },
    ]);
    // Null windows are skipped per key, not dropped entirely.
    expect(h.points("week", 0)).toEqual([{ ms: 0, pct: 5 }]);
  });

  it("filters points by window start", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(0, 10, null), 0);
    h.sample(snapshot(10 * MIN, 20, null), 10 * MIN);
    expect(h.points("five", 5 * MIN)).toEqual([{ ms: 10 * MIN, pct: 20 }]);
  });

  it("ignores error snapshots and near-duplicate fetches", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(0, 10, null), 0);
    h.sample(snapshot(0, 10, null), 5_000); // startup replay of the same poll
    h.sample({ ...snapshot(MIN, 50, null), status: "error" }, MIN);
    expect(h.points("five", 0)).toHaveLength(1);
  });

  it("persists across instances via storage", () => {
    const store = fakeStore();
    new UsageHistory(store).sample(snapshot(0, 10, null), 0);
    expect(new UsageHistory(store).points("five", 0)).toEqual([{ ms: 0, pct: 10 }]);
  });

  it("survives corrupt storage", () => {
    const store = fakeStore({ "usage-history": "not json" });
    const h = new UsageHistory(store);
    expect(h.points("five", 0)).toEqual([]);
  });

  it("thins samples older than six hours to one per five minutes", () => {
    const h = new UsageHistory(fakeStore());
    for (let i = 0; i < 10; i++) h.sample(snapshot(i * MIN, i, null), i * MIN);
    // Jump far ahead; the next sample triggers a prune of the old cluster.
    const later = 7 * 60 * MIN;
    h.sample(snapshot(later, 50, null), later);
    const old = h.points("five", 0).filter((p) => p.ms < 10 * MIN);
    expect(old.length).toBeLessThan(10);
    for (let i = 1; i < old.length; i++) {
      expect(old[i].ms - old[i - 1].ms).toBeGreaterThanOrEqual(5 * MIN);
    }
  });

  it("drops samples older than fifteen days", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(0, 10, null), 0);
    const later = 16 * 24 * 60 * MIN;
    h.sample(snapshot(later, 20, null), later);
    expect(h.points("five", 0)).toEqual([{ ms: later, pct: 20 }]);
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
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null, sec(6 * HOUR)), 3 * HOUR);
    h.sample(snapshot(5 * HOUR, 80, null, sec(6 * HOUR)), 5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    h.sample(snapshot(9 * HOUR, 12, null, sec(13 * HOUR)), 9 * HOUR);
    return h;
  }

  it("segments the previous window by its polled reset time, not wall-clock arithmetic", () => {
    const prev = twoWindows().previousWindow("five", 13 * HOUR, WINDOW);
    expect(prev).not.toBeNull();
    expect(prev!.resetMs).toBe(6 * HOUR);
    expect(prev!.pts).toEqual([
      { ms: 1 * HOUR, pct: 10 },
      { ms: 3 * HOUR, pct: 40 },
      { ms: 5 * HOUR, pct: 80 },
    ]);
  });

  it("excludes the current window's own samples", () => {
    const prev = twoWindows().previousWindow("five", 13 * HOUR, WINDOW)!;
    expect(prev.pts.every((p) => p.ms <= 6 * HOUR)).toBe(true);
  });

  it("treats jittered reset times from mixed sources as one window", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(2 * HOUR, 30, null, sec(6 * HOUR) + 45), 2 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    const prev = h.previousWindow("five", 13 * HOUR, WINDOW)!;
    expect(prev.pts).toHaveLength(2);
  });

  it("returns null when the previous window has fewer than two samples", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(5 * HOUR, 2, null, sec(6 * HOUR)), 5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    expect(h.previousWindow("five", 13 * HOUR, WINDOW)).toBeNull();
  });

  it("returns null for samples from older builds that carry no reset time", () => {
    const h = new UsageHistory(fakeStore());
    h.sample(snapshot(1 * HOUR, 10, null), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null), 3 * HOUR);
    expect(h.previousWindow("five", 13 * HOUR, WINDOW)).toBeNull();
  });

  it("keeps only samples inside the previous window's actual span", () => {
    const h = new UsageHistory(fakeStore());
    // A sample stamped with the old reset but taken after it (stale poll).
    h.sample(snapshot(1 * HOUR, 10, null, sec(6 * HOUR)), 1 * HOUR);
    h.sample(snapshot(3 * HOUR, 40, null, sec(6 * HOUR)), 3 * HOUR);
    h.sample(snapshot(6.5 * HOUR, 40, null, sec(6 * HOUR)), 6.5 * HOUR);
    h.sample(snapshot(8 * HOUR, 5, null, sec(13 * HOUR)), 8 * HOUR);
    const prev = h.previousWindow("five", 13 * HOUR, WINDOW)!;
    expect(prev.pts.map((p) => p.ms)).toEqual([1 * HOUR, 3 * HOUR]);
  });
});
