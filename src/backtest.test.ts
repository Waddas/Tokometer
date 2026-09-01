// Forecast backtest: replays a usage log, and at each step forecasts where
// the window will be `horizon` later, then scores the forecast against what
// the log actually recorded. Runs on the synthetic user from mock.ts (a
// strong weekly rhythm) so the profile model is held to beating the plain
// slope extrapolation, and on a real log when TOKOMETER_HISTORY points at a
// history.json.
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SESSION_ID, WEEKLY_ALL_ID, type HistorySample } from "./api";
import { UsageHistory } from "./history";
import { syntheticLog } from "./mock";
import { interpolate, projectUsage, trendSlope, type Pt } from "./trend";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const RESET_TOLERANCE_MS = 90_000;

interface ModeCfg {
  id: string;
  windowMs: number;
  trendMs: number;
  tauMs: number;
}

// Mirrors MODE in graph.ts.
const SESSION: ModeCfg = { id: SESSION_ID, windowMs: 5 * HOUR, trendMs: 30 * MIN, tauMs: 30 * MIN };
const WEEKLY: ModeCfg = { id: WEEKLY_ALL_ID, windowMs: 7 * DAY, trendMs: 6 * HOUR, tauMs: 2 * HOUR };

/** Mean absolute error in percentage points, per model. `slope` is what the
 *  graph draws with the learned-forecast beta off; `profile` with it on. */
interface Scores {
  n: number;
  flat: number;
  slope: number;
  profile: number;
}

/**
 * Forecast `horizonMs` ahead from roughly every `stepMs`, skipping steps whose
 * window ends before the horizon or that the log stops short of. Only steps
 * after `fromMs` are scored, so the profile has had time to learn.
 */
function backtest(
  samples: HistorySample[],
  cfg: ModeCfg,
  horizonMs: number,
  stepMs: number,
  fromMs: number,
): Scores {
  const err = { flat: 0, slope: 0, profile: 0 };
  let n = 0;
  let nextEval = fromMs;
  const history = new UsageHistory();
  for (let i = 0; i < samples.length; i++) {
    const s = samples[i];
    const win = s.w[cfg.id];
    if (s.ms < nextEval || !win || win.reset == null) continue;
    const target = s.ms + horizonMs;
    if (target > win.reset) continue;
    const actual = actualAt(samples.slice(i), cfg.id, win.reset, target);
    if (actual === null) continue;
    nextEval = s.ms + stepMs;
    n++;

    history.load(samples.slice(0, i + 1));
    const pts = history.points(cfg.id, win.reset - cfg.windowMs, win.reset);
    const momentum = trendSlope(pts, cfg.trendMs, s.ms);
    const proj = projectUsage(s.ms, target, win.pct, {
      momentum,
      momentumTauMs: cfg.tauMs,
      profile: history.rateProfile(cfg.id),
      isWorkDay: () => true,
    });
    err.flat += Math.abs(win.pct - actual);
    err.slope += Math.abs(Math.min(100, win.pct + Math.max(0, momentum ?? 0) * horizonMs) - actual);
    err.profile += Math.abs(proj[proj.length - 1].pct - actual);
  }
  return { n, flat: err.flat / n, slope: err.slope / n, profile: err.profile / n };
}

/** The window's recorded value at `target`, or null if the log doesn't reach it. */
function actualAt(rest: HistorySample[], id: string, resetMs: number, target: number): number | null {
  const pts: Pt[] = [];
  for (const s of rest) {
    const win = s.w[id];
    if (!win || win.reset == null || Math.abs(win.reset - resetMs) > RESET_TOLERANCE_MS) break;
    pts.push({ ms: s.ms, pct: win.pct });
    if (s.ms >= target) break;
  }
  return interpolate(pts, target);
}

function report(label: string, s: Scores): string {
  if (s.n === 0) return `${label.padEnd(14)} no scorable steps (log too sparse)`;
  const f = (x: number) => x.toFixed(2).padStart(6);
  return `${label.padEnd(14)} n=${String(s.n).padStart(4)}  flat ${f(s.flat)}  slope ${f(s.slope)}  profile ${f(s.profile)}`;
}

describe("forecast backtest (synthetic weekly rhythm)", () => {
  const start = new Date(2026, 0, 5).getTime(); // a Monday
  const log = syntheticLog(start, start + 8 * 7 * DAY);
  const scoreFrom = log[0].ms + 3 * 7 * DAY; // three weeks of learning first

  it("beats the plain slope on the 7-day window a day and 4 hours ahead", () => {
    const day = backtest(log, WEEKLY, DAY, 6 * HOUR, scoreFrom);
    const fourHours = backtest(log, WEEKLY, 4 * HOUR, 3 * HOUR, scoreFrom);
    console.log(report("7d +24h", day));
    console.log(report("7d +4h", fourHours));
    expect(day.n).toBeGreaterThan(50);
    expect(day.profile).toBeLessThan(day.slope);
    expect(day.profile).toBeLessThan(day.flat);
    expect(fourHours.profile).toBeLessThan(fourHours.slope);
  });

  it("is no worse than the plain slope on the 5-hour window an hour ahead", () => {
    const hour = backtest(log, SESSION, HOUR, HOUR, scoreFrom);
    console.log(report("5h +1h", hour));
    expect(hour.n).toBeGreaterThan(50);
    expect(hour.profile).toBeLessThanOrEqual(hour.slope);
  });
});

/** history.json entries from builds before per-window ids. */
interface LegacySample {
  ms: number;
  five?: number | null;
  week?: number | null;
  fiveReset?: number | null;
  weekReset?: number | null;
}

function readLog(path: string): HistorySample[] {
  const raw = JSON.parse(readFileSync(path, "utf8")) as Array<HistorySample | LegacySample>;
  return raw.map((s) => {
    if ("w" in s) return s;
    const w: HistorySample["w"] = {};
    if (s.five != null) w[SESSION_ID] = { pct: s.five, reset: s.fiveReset ?? null };
    if (s.week != null) w[WEEKLY_ALL_ID] = { pct: s.week, reset: s.weekReset ?? null };
    return { ms: s.ms, w };
  });
}

// Vitest mirrors the process environment into import.meta.env.
const realLog = import.meta.env.TOKOMETER_HISTORY as string | undefined;

describe.skipIf(!realLog)("forecast backtest (TOKOMETER_HISTORY)", () => {
  it("reports mean absolute error per model", () => {
    const log = readLog(realLog!);
    const from = log[0].ms + Math.min(7 * DAY, (log[log.length - 1].ms - log[0].ms) / 3);
    console.log(`${log.length} samples over ${((log[log.length - 1].ms - log[0].ms) / DAY).toFixed(1)} days`);
    console.log(report("7d +24h", backtest(log, WEEKLY, DAY, 3 * HOUR, from)));
    console.log(report("7d +4h", backtest(log, WEEKLY, 4 * HOUR, HOUR, from)));
    console.log(report("5h +1h", backtest(log, SESSION, HOUR, 30 * MIN, from)));
    console.log(report("5h +30m", backtest(log, SESSION, 30 * MIN, 15 * MIN, from)));
  });
});
