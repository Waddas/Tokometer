import { describe, expect, it } from "vitest";
import { trendSlope, type Pt } from "./trend";

const pt = (ms: number, pct: number): Pt => ({ ms, pct });

describe("trendSlope", () => {
  const MIN = 60_000;

  it("is null with fewer than two recent points", () => {
    expect(trendSlope([], 30 * MIN, 0)).toBeNull();
    expect(trendSlope([pt(0, 10)], 30 * MIN, 0)).toBeNull();
  });

  it("is null until the points span an eighth of the window", () => {
    const pts = [pt(0, 10), pt(2 * MIN, 11)];
    expect(trendSlope(pts, 30 * MIN, 2 * MIN)).toBeNull();
  });

  it("returns percent per ms over the recent points", () => {
    const pts = [pt(0, 10), pt(10 * MIN, 20)];
    expect(trendSlope(pts, 30 * MIN, 10 * MIN)).toBeCloseTo(10 / (10 * MIN));
  });

  it("ignores points older than the span", () => {
    const pts = [pt(0, 0), pt(40 * MIN, 10), pt(60 * MIN, 12)];
    const slope = trendSlope(pts, 30 * MIN, 60 * MIN);
    expect(slope).toBeCloseTo(2 / (20 * MIN));
  });
});
