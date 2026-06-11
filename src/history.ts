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
}

const KEY = "usage-history";
const MAX_AGE_MS = 15 * 86_400_000; // current 7-day window plus the previous one (ghost line)
const DENSE_AGE_MS = 6 * 3_600_000; // keep every sample this recent...
const SPARSE_GAP_MS = 5 * 60_000; // ...thin older ones to one per 5 min
const MIN_GAP_MS = 30_000; // collapse bursts (manual refreshes, replays)

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
