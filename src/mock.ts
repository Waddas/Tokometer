// Dev-only mock data: press M in dev mode to preview the widget with a
// representative usage shape instead of whatever the live account shows.
// Implements the graph's GraphSource interface; never touches localStorage.
import { SESSION_ID, WEEKLY_ALL_ID, type UsageSnapshot } from "./api";
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

/** Which poll to preview: one the oauth endpoint served in full, or a
 *  fallback-probe one whose scoped window is a carried-over last known value
 *  (`carry_missing_windows` in usage.rs). */
export type MockVariant = "fresh" | "stale-scoped";

/** A scoped weekly limit, so the third tile and graph are previewable. */
const SCOPED_ID = "weekly_scoped:fable";

export class MockHistory {
  /** Sample series by window id. */
  private series: Record<string, Pt[]>;
  /** Reset times of the (mock) previous windows by window id, epoch ms. */
  private prevReset: Record<string, number>;
  readonly snapshot: UsageSnapshot;

  constructor(now = Date.now(), variant: MockVariant = "fresh") {
    // 5h window: 3.5h in, heading for a tight finish; busy previous window
    // that lapsed 24 minutes before this one began.
    const fiveEnd = now + 1.5 * HOUR;
    const fiveStart = fiveEnd - 5 * HOUR;
    const prevFiveReset = fiveStart - 0.4 * HOUR;
    const five = [
      ...curve(prevFiveReset - 5 * HOUR, prevFiveReset, 2 * MIN, 88),
      ...curve(fiveStart, now, MIN, 72),
    ];

    // 7d window: 5 days in, comfortable; previous week ran hotter and
    // lapsed 10 hours before this one began. The scoped limit shares the
    // week's boundaries and runs lighter.
    const weekEnd = now + 2 * DAY;
    const weekStart = weekEnd - 7 * DAY;
    const prevWeekReset = weekStart - 10 * HOUR;
    const week = [
      ...curve(prevWeekReset - 7 * DAY, prevWeekReset, 30 * MIN, 61),
      ...curve(weekStart, now, 15 * MIN, 30),
    ];
    const scoped = [
      ...curve(prevWeekReset - 7 * DAY, prevWeekReset, 30 * MIN, 44),
      ...curve(weekStart, now, 15 * MIN, 21),
    ];
    this.series = { [SESSION_ID]: five, [WEEKLY_ALL_ID]: week, [SCOPED_ID]: scoped };
    this.prevReset = {
      [SESSION_ID]: prevFiveReset,
      [WEEKLY_ALL_ID]: prevWeekReset,
      [SCOPED_ID]: prevWeekReset,
    };

    const carried = variant === "stale-scoped";
    this.snapshot = {
      status: "ok",
      // Only the fallback probe ever carries a window over.
      source: carried ? "messages" : "oauth",
      fetchedAt: now,
      windows: [
        {
          id: SESSION_ID,
          label: "5h",
          utilization: five[five.length - 1].pct,
          resetAt: Math.round(fiveEnd / 1000),
        },
        {
          id: WEEKLY_ALL_ID,
          label: "7d",
          utilization: week[week.length - 1].pct,
          resetAt: Math.round(weekEnd / 1000),
        },
        {
          id: SCOPED_ID,
          label: "Fable",
          utilization: scoped[scoped.length - 1].pct,
          resetAt: Math.round(weekEnd / 1000),
          ...(carried ? { stale: true } : {}),
        },
      ],
      error: null,
    };
  }

  points(id: string, startMs: number): Pt[] {
    return (this.series[id] ?? []).filter((p) => p.ms >= startMs);
  }

  previousWindow(id: string, _currentResetMs: number | null, windowMs: number): WindowSlice | null {
    const series = this.series[id];
    const resetMs = this.prevReset[id];
    if (!series || resetMs === undefined) return null;
    const pts = series.filter((p) => p.ms <= resetMs && p.ms >= resetMs - windowMs);
    return pts.length >= 2 ? { pts, resetMs } : null;
  }
}
