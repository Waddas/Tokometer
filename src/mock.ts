// Dev-only mock data: press M in dev mode to preview the widget with a
// representative usage shape instead of whatever the live account shows.
// Implements the graph's GraphSource interface; never touches localStorage.
import type { UsageSnapshot } from "./api";
import type { WindowSlice } from "./history";
import type { Pt } from "./trend";

const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/**
 * A plausible usage curve: idle stretches broken by bursts of activity,
 * scaled so the last sample lands on `target` percent.
 */
function curve(start: number, end: number, step: number, target: number): Pt[] {
  const raw: number[] = [0];
  let v = 0;
  let burst = 0;
  for (let t = start + step; t <= end; t += step) {
    if (burst > 0) {
      v += 2 + Math.random() * 3;
      burst--;
    } else if (Math.random() < 0.08) {
      burst = 2 + Math.floor(Math.random() * 6);
    } else {
      v += Math.random() * 0.3;
    }
    raw.push(v);
  }
  const scale = target / raw[raw.length - 1];
  return raw.map((p, i) => ({ ms: start + i * step, pct: Math.min(100, p * scale) }));
}

export class MockHistory {
  private five: Pt[];
  private week: Pt[];
  /** Reset times of the (mock) previous windows, epoch ms. */
  private prevFiveReset: number;
  private prevWeekReset: number;
  readonly snapshot: UsageSnapshot;

  constructor(now = Date.now()) {
    // 5h window: 3.5h in, heading for a tight finish; busy previous window
    // that lapsed 24 minutes before this one began.
    const fiveEnd = now + 1.5 * HOUR;
    const fiveStart = fiveEnd - 5 * HOUR;
    this.prevFiveReset = fiveStart - 0.4 * HOUR;
    this.five = [
      ...curve(this.prevFiveReset - 5 * HOUR, this.prevFiveReset, 2 * MIN, 88),
      ...curve(fiveStart, now, MIN, 72),
    ];

    // 7d window: 5 days in, comfortable; previous week ran hotter and
    // lapsed 10 hours before this one began.
    const weekEnd = now + 2 * DAY;
    const weekStart = weekEnd - 7 * DAY;
    this.prevWeekReset = weekStart - 10 * HOUR;
    this.week = [
      ...curve(this.prevWeekReset - 7 * DAY, this.prevWeekReset, 30 * MIN, 61),
      ...curve(weekStart, now, 15 * MIN, 30),
    ];

    this.snapshot = {
      status: "ok",
      source: "oauth",
      fetchedAt: now,
      fiveHour: {
        utilization: this.five[this.five.length - 1].pct,
        resetAt: Math.round(fiveEnd / 1000),
      },
      sevenDay: {
        utilization: this.week[this.week.length - 1].pct,
        resetAt: Math.round(weekEnd / 1000),
      },
      fiveHourStatus: null,
      error: null,
    };
  }

  points(key: "five" | "week", startMs: number): Pt[] {
    return (key === "five" ? this.five : this.week).filter((p) => p.ms >= startMs);
  }

  previousWindow(key: "five" | "week", _currentResetMs: number, windowMs: number): WindowSlice | null {
    const resetMs = key === "five" ? this.prevFiveReset : this.prevWeekReset;
    const pts = (key === "five" ? this.five : this.week).filter(
      (p) => p.ms <= resetMs && p.ms >= resetMs - windowMs,
    );
    return pts.length >= 2 ? { pts, resetMs } : null;
  }
}
