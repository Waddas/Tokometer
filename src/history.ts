// Usage-over-time sample log behind the graph. The backend only keeps the
// latest snapshot, so the frontend accumulates one here, in localStorage.
import type { UsageSnapshot } from "./api";
import type { Pt } from "./trend";

interface Sample {
  /** unix epoch ms */
  ms: number;
  /** 0-100 percent, null when the poll lacked that window */
  five: number | null;
  week: number | null;
  /** each window's reset time (epoch ms); absent on samples from older builds */
  fiveReset?: number | null;
  weekReset?: number | null;
}

/** A completed usage window's samples, plus the reset time that identifies it. */
export interface WindowSlice {
  pts: Pt[];
  /** unix epoch ms */
  resetMs: number;
}

const KEY = "usage-history";
const MAX_AGE_MS = 15 * 86_400_000; // current 7-day window plus the previous one (ghost line)
const DENSE_AGE_MS = 6 * 3_600_000; // keep every sample this recent...
const SPARSE_GAP_MS = 5 * 60_000; // ...thin older ones to one per 5 min
const MIN_GAP_MS = 30_000; // collapse bursts (manual refreshes, replays)
// Polls whose reset times are this close report the same window — absorbs
// second-level jitter between the OAuth body and rate-limit-header sources.
const RESET_TOLERANCE_MS = 90_000;

type Store = Pick<Storage, "getItem" | "setItem">;

export class UsageHistory {
  private samples: Sample[] = [];
  private storage: Store | null;

  constructor(storage: Store | null = globalThis.localStorage ?? null) {
    this.storage = storage;
    try {
      const raw = this.storage?.getItem(KEY);
      if (raw) this.samples = JSON.parse(raw) as Sample[];
    } catch {
      this.samples = [];
    }
  }

  /** Record a snapshot at its fetch time; near-duplicates are dropped. */
  sample(s: UsageSnapshot, now = Date.now()): void {
    if (s.status !== "ok") return;
    const ms = s.fetchedAt || now;
    const last = this.samples[this.samples.length - 1];
    if (last && ms - last.ms < MIN_GAP_MS) return;
    this.samples.push({
      ms,
      five: s.fiveHour?.utilization ?? null,
      week: s.sevenDay?.utilization ?? null,
      fiveReset: s.fiveHour?.resetAt != null ? s.fiveHour.resetAt * 1000 : null,
      weekReset: s.sevenDay?.resetAt != null ? s.sevenDay.resetAt * 1000 : null,
    });
    this.prune(now);
    try {
      this.storage?.setItem(KEY, JSON.stringify(this.samples));
    } catch {
      // Quota or private mode: the in-memory log still works.
    }
  }

  /** Points for one usage window since `startMs`, oldest first. */
  points(key: "five" | "week", startMs: number): Pt[] {
    const pts: Pt[] = [];
    for (const s of this.samples) {
      const pct = s[key];
      if (s.ms >= startMs && pct !== null) pts.push({ ms: s.ms, pct });
    }
    return pts;
  }

  /**
   * The most recent window that completed before the current one, segmented
   * by the reset time each sample was polled with — windows start whenever
   * the first message after a lapse lands, so wall-clock arithmetic can't
   * find their boundaries. Returns null when history holds no such window
   * with at least two points (samples from older builds carry no reset time
   * and are never segmented).
   */
  previousWindow(
    key: "five" | "week",
    currentResetMs: number,
    windowMs: number,
  ): WindowSlice | null {
    const resetKey = key === "five" ? "fiveReset" : "weekReset";
    let resetMs: number | null = null;
    for (let i = this.samples.length - 1; i >= 0; i--) {
      const r = this.samples[i][resetKey];
      if (r == null || this.samples[i][key] === null) continue;
      if (r < currentResetMs - RESET_TOLERANCE_MS) {
        resetMs = r;
        break;
      }
    }
    if (resetMs === null) return null;
    const pts: Pt[] = [];
    for (const s of this.samples) {
      const r = s[resetKey];
      const pct = s[key];
      if (r == null || pct === null) continue;
      if (Math.abs(r - resetMs) > RESET_TOLERANCE_MS) continue;
      // The span check inherits the anchor's jitter, so give it the same slack.
      if (s.ms > resetMs || s.ms < resetMs - windowMs - RESET_TOLERANCE_MS) continue;
      pts.push({ ms: s.ms, pct });
    }
    return pts.length >= 2 ? { pts, resetMs } : null;
  }

  private prune(now: number): void {
    const kept: Sample[] = [];
    for (const s of this.samples) {
      const age = now - s.ms;
      if (age > MAX_AGE_MS) continue;
      const last = kept[kept.length - 1];
      if (age > DENSE_AGE_MS && last && s.ms - last.ms < SPARSE_GAP_MS) continue;
      kept.push(s);
    }
    this.samples = kept;
  }
}
