// Dev-only mock data: press M in dev mode to preview the widget with a
// representative usage shape instead of whatever the live account shows.
// Implements the graph's GraphSource interface; never touches localStorage.
// Also home to the synthetic user the forecast backtest replays, so the
// preview's learned profile and the test's training data are the same thing.
import { SESSION_ID, WEEKLY_ALL_ID, type HistorySample, type UsageSnapshot } from "./api";
import { UsageHistory, type WindowSlice } from "./history";
import type { Pt, RateProfile } from "./trend";

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

/** Deterministic LCG in [0, 1). */
function rng(seed: number): () => number {
  let x = seed >>> 0;
  return () => {
    x = (x * 1_664_525 + 1_013_904_223) >>> 0;
    return x / 2 ** 32;
  };
}

/**
 * A developer with a weekly rhythm: office hours Mon–Fri with a lunch dip,
 * a little in the evening, a quiet Saturday afternoon, bursty within the
 * hour. Weekly windows run back to back from the first Monday; a 5h window
 * opens on the first usage after a lapse. Percentages are integers like the
 * API's. `intensity` scales every rate (1 ≈ 40% of the weekly limit a week).
 */
export function syntheticLog(startMs: number, endMs: number, intensity = 1, seed = 7): HistorySample[] {
  const random = rng(seed);
  const step = 15 * MIN;
  const samples: HistorySample[] = [];
  let week = 0;
  let weekReset = startMs + 7 * DAY;
  let session = 0;
  let sessionReset: number | null = null;
  let burst = 0;
  for (let t = startMs; t < endMs; t += step) {
    const d = new Date(t);
    const day = d.getDay();
    const hour = d.getHours() + d.getMinutes() / 60;
    const weekday = day >= 1 && day <= 5;
    let perHour = 0;
    if (weekday && hour >= 9 && hour < 18) perHour = hour >= 12 && hour < 13 ? 0.15 : 0.9;
    else if (weekday && hour >= 19 && hour < 22) perHour = 0.25;
    else if (day === 6 && hour >= 13 && hour < 17) perHour = 0.3;
    if (burst > 0) burst--;
    else if (perHour > 0 && random() < 0.12) burst = 1 + Math.floor(random() * 4);
    const factor = burst > 0 ? 2.5 : 0.6 + random() * 0.8;
    const gain = (intensity * perHour * factor * step) / HOUR;

    if (t >= weekReset) {
      week = 0;
      weekReset += 7 * DAY;
    }
    if (sessionReset !== null && t >= sessionReset) {
      session = 0;
      sessionReset = null;
    }
    if (gain > 0 && sessionReset === null) sessionReset = t + 5 * HOUR;
    week = Math.min(100, week + gain);
    if (sessionReset !== null) session = Math.min(100, session + gain * 11);

    samples.push({
      ms: t,
      w: {
        [SESSION_ID]: { pct: Math.round(session), reset: sessionReset },
        [WEEKLY_ALL_ID]: { pct: Math.round(week), reset: weekReset },
      },
    });
  }
  return samples;
}

/** The Monday 00:00 local at least `weeks` weeks before `ms`. */
function mondayBefore(ms: number, weeks: number): number {
  const d = new Date(ms - weeks * 7 * DAY);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d.getTime();
}

/** Which poll to preview: one the oauth endpoint served in full, a
 *  fallback-probe one whose scoped window is a carried-over last known value
 *  (`carry_missing_windows` in usage.rs), or a quiet account barely using
 *  either window. */
export type MockVariant = "fresh" | "stale-scoped" | "quiet";

// How far each variant's windows have got: current and previous 5h and 7d.
const SHAPES = {
  fresh: { five: 72, prevFive: 88, week: 30, prevWeek: 61, scoped: 21, intensity: 1 },
  "stale-scoped": { five: 72, prevFive: 88, week: 30, prevWeek: 61, scoped: 21, intensity: 1 },
  quiet: { five: 14, prevFive: 22, week: 9, prevWeek: 15, scoped: 6, intensity: 0.3 },
} as const;

export class MockHistory {
  private five: Pt[];
  private week: Pt[];
  /** Reset times of the (mock) previous windows, epoch ms. */
  private prevFiveReset: number;
  private prevWeekReset: number;
  /** Five weeks of the synthetic user, for a realistic learned forecast. */
  private learned = new UsageHistory();
  readonly snapshot: UsageSnapshot;

  constructor(now = Date.now(), variant: MockVariant = "fresh") {
    const shape = SHAPES[variant];
    // 5h window: 3.5h in; previous window lapsed 24 minutes before this one began.
    const fiveEnd = now + 1.5 * HOUR;
    const fiveStart = fiveEnd - 5 * HOUR;
    this.prevFiveReset = fiveStart - 0.4 * HOUR;
    this.five = [
      ...curve(this.prevFiveReset - 5 * HOUR, this.prevFiveReset, 2 * MIN, shape.prevFive),
      ...curve(fiveStart, now, MIN, shape.five),
    ];

    // 7d window: 5 days in; previous week lapsed 10 hours before this one began.
    const weekEnd = now + 2 * DAY;
    const weekStart = weekEnd - 7 * DAY;
    this.prevWeekReset = weekStart - 10 * HOUR;
    this.week = [
      ...curve(this.prevWeekReset - 7 * DAY, this.prevWeekReset, 30 * MIN, shape.prevWeek),
      ...curve(weekStart, now, 15 * MIN, shape.week),
    ];

    this.learned.load(syntheticLog(mondayBefore(now, 5), now, shape.intensity));

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
          utilization: this.five[this.five.length - 1].pct,
          resetAt: Math.round(fiveEnd / 1000),
        },
        {
          id: WEEKLY_ALL_ID,
          label: "7d",
          utilization: this.week[this.week.length - 1].pct,
          resetAt: Math.round(weekEnd / 1000),
        },
        // A scoped limit, so the third tile is previewable.
        {
          id: "weekly_scoped:fable",
          label: "Fable",
          utilization: shape.scoped,
          resetAt: Math.round(weekEnd / 1000),
          ...(carried ? { stale: true } : {}),
        },
      ],
      error: null,
    };
  }

  /** The mock only graphs the two windows the graph has modes for. */
  private series(id: string): Pt[] | null {
    if (id === SESSION_ID) return this.five;
    if (id === WEEKLY_ALL_ID) return this.week;
    return null;
  }

  points(id: string, startMs: number): Pt[] {
    return (this.series(id) ?? []).filter((p) => p.ms >= startMs);
  }

  previousWindow(id: string, _currentResetMs: number | null, windowMs: number): WindowSlice | null {
    const series = this.series(id);
    if (!series) return null;
    const resetMs = id === SESSION_ID ? this.prevFiveReset : this.prevWeekReset;
    const pts = series.filter((p) => p.ms <= resetMs && p.ms >= resetMs - windowMs);
    return pts.length >= 2 ? { pts, resetMs } : null;
  }

  rateProfile(id: string): RateProfile {
    return this.learned.rateProfile(id);
  }
}
