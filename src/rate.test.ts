import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RateTracker } from "./rate";

// RateTracker reads Date.now(), so drive time with fake timers. Each helper
// advances the clock then feeds a sample, mirroring how the poller calls it.
const MINUTE = 60_000;

describe("RateTracker", () => {
  let tracker: RateTracker;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    tracker = new RateTracker();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  /** Advance the clock by `mins` minutes, then record `pct`. */
  function sampleAfter(mins: number, pct: number): void {
    vi.advanceTimersByTime(mins * MINUTE);
    tracker.sample(pct);
  }

  it("reports Idle until at least two samples exist", () => {
    expect(tracker.group()).toBe(0);
    tracker.sample(10);
    expect(tracker.group()).toBe(0);
  });

  it("stays Idle while the history window is shorter than the warm-up (4 min)", () => {
    tracker.sample(0);
    // 3 minutes of history — below MIN_WINDOW_MS even at a steep rate.
    sampleAfter(3, 100);
    expect(tracker.group()).toBe(0);
  });

  it("classifies a flat session as Idle once warmed up", () => {
    tracker.sample(50);
    sampleAfter(5, 50);
    expect(tracker.group()).toBe(0);
  });

  it("classifies ~0.15 %/min as Normal (1)", () => {
    tracker.sample(0);
    // 0.75% over 5 min = 0.15 %/min → between 0.10 and 0.20.
    sampleAfter(5, 0.75);
    expect(tracker.group()).toBe(1);
  });

  it("classifies ~0.25 %/min as Active (2)", () => {
    tracker.sample(0);
    // 1.25% over 5 min = 0.25 %/min → between 0.20 and 0.33.
    sampleAfter(5, 1.25);
    expect(tracker.group()).toBe(2);
  });

  it("classifies the reset-pace rate (0.33 %/min) as Heavy (3)", () => {
    tracker.sample(0);
    // Exactly the 5-hour fill pace: 100% / 300 min ≈ 1.65% over 5 min.
    sampleAfter(5, 1.65);
    expect(tracker.group()).toBe(3);
  });

  it("treats a decreasing percentage as zero rate (Idle), never negative", () => {
    tracker.sample(40);
    // A small dip (within the 5-point reset threshold) clamps to 0 rate.
    sampleAfter(5, 38);
    expect(tracker.group()).toBe(0);
  });

  it("restarts tracking when the session resets (pct drops >5 points)", () => {
    tracker.sample(80);
    sampleAfter(5, 82);
    expect(tracker.group()).toBeGreaterThan(0);

    // Session reset: percentage falls well below the previous sample.
    sampleAfter(1, 10);
    // Ring was cleared, so only one fresh sample remains → Idle.
    expect(tracker.group()).toBe(0);
  });

  it("keeps only the last RING_SIZE (6) samples, sliding the window forward", () => {
    // Seven steadily-rising samples one minute apart. The oldest is evicted,
    // so the measured window is the last 6 (5 minutes), not all 7.
    for (let i = 0; i < 7; i++) sampleAfter(1, i * 1);
    // Last six span pct 1→6 over 5 min = 1 %/min → Heavy.
    expect(tracker.group()).toBe(3);
  });

  it("computes the rate across the full retained window, not just the last delta", () => {
    tracker.sample(0);
    sampleAfter(2, 0.2);
    sampleAfter(2, 0.4);
    // 0.4% over 4 min = 0.10 %/min, which is the Normal threshold boundary.
    expect(tracker.group()).toBe(1);
  });
});
