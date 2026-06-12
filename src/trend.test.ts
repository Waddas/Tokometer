import { describe, expect, it } from "vitest";
import { trendSlope, projectUsage, type Pt } from "./trend";

const pt = (ms: number, pct: number): Pt => ({ ms, pct });

const DAY = 86_400_000;
const last = (pts: Pt[]) => pts[pts.length - 1];

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

describe("projectUsage", () => {
  const all = () => true;
  // Monday 00:00 local, so day boundaries land on whole work/rest days.
  const monday = new Date(2026, 0, 5).getTime();
  const weekdays = (ms: number) => {
    const d = new Date(ms).getDay();
    return d !== 0 && d !== 6;
  };

  it("always lands a final vertex exactly at `end`", () => {
    expect(last(projectUsage(0, 3 * DAY, 0, 10 / DAY, all)).ms).toBe(3 * DAY);
    expect(last(projectUsage(0, 3 * DAY, 0, 0, all)).ms).toBe(3 * DAY);
  });

  it("with every day working, ramps the full span like a constant rate", () => {
    const proj = projectUsage(monday, monday + 7 * DAY, 0, 10 / DAY, all);
    expect(proj[0].pct).toBe(0);
    expect(last(proj).pct).toBeCloseTo(70);
  });

  it("holds flat across non-work days", () => {
    // Mon–Fri ramp at 10%/day, Sat/Sun flat: 5 working days → 50%.
    const proj = projectUsage(monday, monday + 7 * DAY, 0, 10 / DAY, weekdays);
    expect(last(proj).pct).toBeCloseTo(50);
    // Saturday's segment (day 5 → day 6) neither rises nor falls.
    const satStart = proj.find((p) => p.ms === monday + 5 * DAY);
    const sunStart = proj.find((p) => p.ms === monday + 6 * DAY);
    expect(satStart?.pct).toBeCloseTo(50);
    expect(sunStart?.pct).toBeCloseTo(50);
  });

  it("caps at 100% and stops once the limit is reached", () => {
    const proj = projectUsage(monday, monday + 7 * DAY, 80, 40 / DAY, weekdays);
    expect(last(proj).pct).toBe(100);
    expect(Math.max(...proj.map((p) => p.pct))).toBe(100);
    expect(last(proj).ms).toBe(monday + 7 * DAY);
  });

  it("stays flat when the rate is zero or already full", () => {
    expect(projectUsage(0, DAY, 42, 0, all)).toEqual([pt(0, 42), pt(DAY, 42)]);
    expect(projectUsage(0, DAY, 100, 10 / DAY, all)).toEqual([pt(0, 100), pt(DAY, 100)]);
  });
});
