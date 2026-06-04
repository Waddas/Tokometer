// Port of Clawdmeter's firmware usage_rate.cpp: smooths session-% deltas over
// a ring buffer so a single noisy sample can't spike the animation group.
//
// Thresholds in %/min — 0.33 %/min fills a 5-hour session exactly at reset pace:
//   < 0.10  → 0 Idle    (17h+ to fill)
//   < 0.20  → 1 Normal  (8–17h)
//   < 0.33  → 2 Active  (5–8h)
//   >=0.33  → 3 Heavy   (≤5h, matching the session reset)
const RATE_THRESH_NORMAL = 0.1;
const RATE_THRESH_ACTIVE = 0.2;
const RATE_THRESH_HEAVY = 0.33;
// ~4 min of history required before the rate is trusted (warm-up reports Idle).
const MIN_WINDOW_MS = 240_000;
const RING_SIZE = 6;

interface Sample {
  ms: number;
  pct: number;
}

export class RateTracker {
  private ring: Sample[] = [];

  sample(sessionPct: number): void {
    const last = this.ring[this.ring.length - 1];
    // Session reset: pct dropped substantially. Restart tracking.
    if (last && sessionPct + 5 < last.pct) this.ring = [];
    this.ring.push({ ms: Date.now(), pct: sessionPct });
    if (this.ring.length > RING_SIZE) this.ring.shift();
  }

  group(): number {
    if (this.ring.length < 2) return 0;
    const oldest = this.ring[0];
    const latest = this.ring[this.ring.length - 1];
    const dt = latest.ms - oldest.ms;
    if (dt < MIN_WINDOW_MS) return 0;

    const dp = Math.max(0, latest.pct - oldest.pct);
    const rate = (dp * 60_000) / dt;

    if (rate < RATE_THRESH_NORMAL) return 0;
    if (rate < RATE_THRESH_ACTIVE) return 1;
    if (rate < RATE_THRESH_HEAVY) return 2;
    return 3;
  }
}
