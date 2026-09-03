// In-memory view of the usage-over-time log behind the graph. The backend
// poller owns recording, retention and persistence (history.rs); this mirrors
// its log at startup and appends live snapshots as they stream in, so the
// graph never waits on a round-trip.
import type { HistorySample, UsageSnapshot } from "./api";
import type { Pt } from "./trend";

/** A completed usage window's samples, plus the reset time that identifies it. */
export interface WindowSlice {
  pts: Pt[];
  /** unix epoch ms */
  resetMs: number;
}

const MIN_GAP_MS = 30_000; // collapse bursts (manual refreshes, replays)
// Polls whose reset times are this close report the same window — absorbs
// second-level jitter between the OAuth body and rate-limit-header sources.
const RESET_TOLERANCE_MS = 90_000;

export class UsageHistory {
  private samples: HistorySample[] = [];

  /** Replace the log with the backend's (startup, or after migration). */
  load(samples: HistorySample[]): void {
    this.samples = [...samples];
  }

  /** Record a snapshot at its fetch time; near-duplicates are dropped. */
  sample(s: UsageSnapshot, now = Date.now()): void {
    if (s.status !== "ok") return;
    const ms = s.fetchedAt || now;
    const last = this.samples[this.samples.length - 1];
    if (last && ms - last.ms < MIN_GAP_MS) return;
    const w: HistorySample["w"] = {};
    for (const win of s.windows) {
      w[win.id] = { pct: win.utilization, reset: win.resetAt != null ? win.resetAt * 1000 : null };
    }
    this.samples.push({ ms, w });
  }

  /**
   * Points for one usage window (by id) since `startMs`, oldest first. Samples
   * stamped with a different reset time belong to another window and are
   * dropped: after a window lapses (`currentResetMs` null, so nothing is
   * running) every stamped sample is some finished window's, and only the
   * unstamped zero-usage polls of the lapse itself remain. Samples from
   * older builds carry no reset time and are kept on the time filter alone.
   */
  points(id: string, startMs: number, currentResetMs: number | null): Pt[] {
    const pts: Pt[] = [];
    for (const s of this.samples) {
      const win = s.w[id];
      if (s.ms < startMs || !win) continue;
      const r = win.reset;
      if (r != null) {
        if (currentResetMs === null) continue;
        if (Math.abs(r - currentResetMs) > RESET_TOLERANCE_MS) continue;
      }
      pts.push({ ms: s.ms, pct: win.pct });
    }
    return pts;
  }

  /**
   * The most recent window that completed before the current one, segmented
   * by the reset time each sample was polled with — windows start whenever
   * the first message after a lapse lands, so wall-clock arithmetic can't
   * find their boundaries. A null `currentResetMs` means no window is
   * running (it lapsed), so the latest recorded window is the previous one.
   * Returns null when history holds no such window with at least two points
   * (samples from older builds carry no reset time and are never segmented).
   */
  previousWindow(id: string, currentResetMs: number | null, windowMs: number): WindowSlice | null {
    let resetMs: number | null = null;
    for (let i = this.samples.length - 1; i >= 0; i--) {
      const r = this.samples[i].w[id]?.reset;
      if (r == null) continue;
      if (currentResetMs === null || r < currentResetMs - RESET_TOLERANCE_MS) {
        resetMs = r;
        break;
      }
    }
    if (resetMs === null) return null;
    const pts: Pt[] = [];
    for (const s of this.samples) {
      const win = s.w[id];
      if (!win || win.reset == null) continue;
      if (Math.abs(win.reset - resetMs) > RESET_TOLERANCE_MS) continue;
      // The span check inherits the anchor's jitter, so give it the same slack.
      if (s.ms > resetMs || s.ms < resetMs - windowMs - RESET_TOLERANCE_MS) continue;
      pts.push({ ms: s.ms, pct: win.pct });
    }
    return pts.length >= 2 ? { pts, resetMs } : null;
  }
}
