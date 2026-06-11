import { describe, expect, it } from "vitest";
import { band, colorSegments, trendSlope, type Pt } from "./trend";

const pt = (ms: number, pct: number): Pt => ({ ms, pct });

describe("band", () => {
  it("maps the usage thresholds", () => {
    expect(band(0)).toBe(0);
    expect(band(49.9)).toBe(0);
    expect(band(50)).toBe(1);
    expect(band(79.9)).toBe(1);
    expect(band(80)).toBe(2);
    expect(band(100)).toBe(2);
  });
});

describe("colorSegments", () => {
  it("returns nothing for no points", () => {
    expect(colorSegments([])).toEqual([]);
  });

  it("keeps a single-band line as one run", () => {
    const runs = colorSegments([pt(0, 10), pt(100, 20), pt(200, 30)]);
    expect(runs).toHaveLength(1);
    expect(runs[0].band).toBe(0);
    expect(runs[0].pts).toHaveLength(3);
  });

  it("splits at a threshold crossing with an interpolated point", () => {
    const runs = colorSegments([pt(0, 40), pt(100, 60)]);
    expect(runs).toHaveLength(2);
    expect(runs[0].band).toBe(0);
    expect(runs[1].band).toBe(1);
    // Crossing at pct 50 is halfway between 40 and 60.
    expect(runs[0].pts[1]).toEqual({ ms: 50, pct: 50 });
    // Runs share the boundary point so the strokes connect.
    expect(runs[1].pts[0]).toEqual(runs[0].pts[1]);
  });

  it("crosses two thresholds in one segment", () => {
    const runs = colorSegments([pt(0, 40), pt(100, 90)]);
    expect(runs.map((r) => r.band)).toEqual([0, 1, 2]);
    expect(runs[0].pts[1].pct).toBe(50);
    expect(runs[1].pts[1].pct).toBe(80);
  });

  it("splits when crossing back down", () => {
    const runs = colorSegments([pt(0, 60), pt(100, 40)]);
    expect(runs.map((r) => r.band)).toEqual([1, 0]);
    expect(runs[0].pts[1].pct).toBe(50);
  });
});

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
