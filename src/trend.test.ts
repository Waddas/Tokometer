import { describe, expect, it } from "vitest";
import {
  RateProfile,
  interpolate,
  localMidnights,
  projectUsage,
  straighten,
  trendSlope,
  type ForecastModel,
  type Gain,
  type Pt,
} from "./trend";

const pt = (ms: number, pct: number): Pt => ({ ms, pct });

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const last = (pts: Pt[]) => pts[pts.length - 1];

// Monday 00:00 local, so day boundaries land on whole work/rest days.
const monday = new Date(2026, 0, 5).getTime();
const at = (day: number, hour: number) => monday + day * DAY + hour * HOUR;

/** Hourly gains at `ratePerHour` across `days` days from Monday. */
function steadyGains(ratePerHour: number, days = 7): Gain[] {
  return Array.from({ length: days * 24 }, (_, h) => ({
    fromMs: monday + h * HOUR,
    toMs: monday + (h + 1) * HOUR,
    pct: ratePerHour,
  }));
}

/** Office hours only: 9–17 Mon–Fri at `ratePerHour`, nothing otherwise. */
function officeHoursGains(ratePerHour: number, weeks: number): Gain[] {
  return steadyGains(0, weeks * 7).map((g) => {
    const d = new Date(g.fromMs);
    const office = d.getDay() >= 1 && d.getDay() <= 5 && d.getHours() >= 9 && d.getHours() < 17;
    return office ? { ...g, pct: ratePerHour } : g;
  });
}

function model(overrides: Partial<ForecastModel>): ForecastModel {
  return {
    momentum: null,
    momentumTauMs: HOUR,
    profile: new RateProfile(),
    isWorkDay: () => true,
    ...overrides,
  };
}

describe("trendSlope", () => {
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

describe("RateProfile", () => {
  it("has no data and a zero rate until a gain is added", () => {
    const p = new RateProfile();
    expect(p.hasData).toBe(false);
    expect(p.rateAt(at(1, 12))).toBe(0);
  });

  it("reproduces a steady rate exactly at every hour", () => {
    const p = RateProfile.from(steadyGains(2));
    for (let h = 0; h < 7 * 24; h += 5) {
      expect(p.rateAt(monday + h * HOUR + 17 * MIN) * HOUR).toBeCloseTo(2);
    }
  });

  it("spreads one gain across the hours it spans", () => {
    // 4% between 09:30 and 11:30, on top of a quiet baseline week so the
    // priors stay near zero: the middle hour saw the whole 2%/h, the ends half.
    const p = RateProfile.from([
      ...steadyGains(0),
      { fromMs: at(0, 9.5), toMs: at(0, 11.5), pct: 4 },
    ]);
    const r = (hour: number) => p.rateAt(at(0, hour)) * HOUR;
    expect(r(10)).toBeGreaterThan(r(9));
    expect(r(9)).toBeCloseTo(r(11));
    expect(r(11)).toBeGreaterThan(r(12));
    expect(r(12)).toBeCloseTo(0, 1);
  });

  it("ignores negative gains and gaps longer than a day", () => {
    const p = RateProfile.from([{ fromMs: at(0, 9), toMs: at(0, 10), pct: -5 }]);
    expect(p.hasData).toBe(true);
    expect(p.rateAt(at(0, 9))).toBe(0);
    const gap = RateProfile.from([{ fromMs: at(0, 0), toMs: at(1, 1), pct: 50 }]);
    expect(gap.hasData).toBe(false);
  });

  it("learns a weekly shape: busy office hours, quiet nights and weekends", () => {
    const p = RateProfile.from(officeHoursGains(1, 2));
    const r = (day: number, hour: number) => p.rateAt(at(day, hour)) * HOUR;
    expect(r(1, 12)).toBeGreaterThan(0.75); // Tuesday noon
    expect(r(1, 3)).toBeLessThan(0.1); // Tuesday 3am
    expect(r(5, 12)).toBeLessThan(0.1); // Saturday noon
  });

  it("shrinks an unseen hour toward its day and hour effects rather than zero", () => {
    // Two weeks of office hours, but Wednesday 14:00 was never observed.
    const gains = officeHoursGains(1, 2).filter((g) => {
      const d = new Date(g.fromMs);
      return !(d.getDay() === 3 && d.getHours() === 14);
    });
    const p = RateProfile.from(gains);
    expect(p.rateAt(at(2, 14)) * HOUR).toBeGreaterThan(0.5);
  });
});

describe("projectUsage", () => {
  const weekdays = (ms: number) => {
    const d = new Date(ms).getDay();
    return d !== 0 && d !== 6;
  };
  const steady = (ratePerDay: number) => RateProfile.from(steadyGains(ratePerDay / 24));

  it("always lands a final vertex exactly at `end`", () => {
    expect(last(projectUsage(0, 3 * DAY, 0, model({ profile: steady(10) }))).ms).toBe(3 * DAY);
    expect(last(projectUsage(0, 3 * DAY, 0, model({}))).ms).toBe(3 * DAY);
  });

  it("with every day working, ramps the full span at the profile's rate", () => {
    const proj = projectUsage(monday, monday + 7 * DAY, 0, model({ profile: steady(10) }));
    expect(proj[0].pct).toBe(0);
    expect(last(proj).pct).toBeCloseTo(70);
  });

  it("holds flat across non-work days", () => {
    // Mon–Fri ramp at 10%/day, Sat/Sun flat: 5 working days → 50%.
    const proj = projectUsage(
      monday,
      monday + 7 * DAY,
      0,
      model({ profile: steady(10), isWorkDay: weekdays }),
    );
    expect(last(proj).pct).toBeCloseTo(50);
    const satStart = proj.find((p) => p.ms === monday + 5 * DAY);
    const sunStart = proj.find((p) => p.ms === monday + 6 * DAY);
    expect(satStart?.pct).toBeCloseTo(50);
    expect(sunStart?.pct).toBeCloseTo(50);
  });

  it("caps at 100% and stops once the limit is reached", () => {
    const proj = projectUsage(
      monday,
      monday + 7 * DAY,
      80,
      model({ profile: steady(40), isWorkDay: weekdays }),
    );
    expect(last(proj).pct).toBe(100);
    expect(Math.max(...proj.map((p) => p.pct))).toBe(100);
    expect(last(proj).ms).toBe(monday + 7 * DAY);
    // Reached the limit half a day in, then held.
    const hit = proj.find((p) => p.pct === 100)!;
    expect(hit.ms).toBeCloseTo(monday + 0.5 * DAY, -6);
  });

  it("stays flat when nothing is known or already full", () => {
    const flat = projectUsage(0, DAY, 42, model({}));
    expect(flat.every((p) => p.pct === 42)).toBe(true);
    expect(last(flat).ms).toBe(DAY);
    expect(projectUsage(0, DAY, 100, model({ profile: steady(10) }))).toEqual([
      pt(0, 100),
      pt(DAY, 100),
    ]);
  });

  it("follows momentum first and lets it decay over tau", () => {
    // 10%/h of momentum with a 1h tau and no profile adds ~10% in total.
    const proj = projectUsage(monday, monday + DAY, 0, model({ momentum: 10 / HOUR }));
    expect(last(proj).pct).toBeCloseTo(10, 3);
    // Most of it lands in the first hour.
    const afterHour = interpolate(proj, monday + HOUR)!;
    expect(afterHour).toBeCloseTo(10 * (1 - Math.exp(-1)), 3);
  });

  it("hands over from momentum to the profile", () => {
    const proj = projectUsage(
      monday,
      monday + DAY,
      0,
      model({ momentum: 10 / HOUR, profile: steady(24) }),
    );
    // Momentum contributes 10·(1−e⁻²⁴), the profile 1%/h over the rest.
    const momentumHours = 1 - Math.exp(-24);
    expect(last(proj).pct).toBeCloseTo(10 * momentumHours + (24 - momentumHours), 3);
  });

  it("keeps momentum as a constant rate when tau is infinite (beta off)", () => {
    const proj = projectUsage(
      monday,
      monday + 7 * DAY,
      0,
      model({ momentum: 10 / DAY, momentumTauMs: Infinity, profile: steady(24), isWorkDay: weekdays }),
    );
    // 5 working days at 10%/day; the profile is never consulted.
    expect(last(proj).pct).toBeCloseTo(50);
  });

  it("never slopes downward, even with negative momentum", () => {
    const proj = projectUsage(0, DAY, 42, model({ momentum: -10 / HOUR }));
    expect(proj.every((p) => p.pct === 42)).toBe(true);
  });
});

describe("straighten", () => {
  const weekdays = (ms: number) => {
    const d = new Date(ms).getDay();
    return d !== 0 && d !== 6;
  };
  const steady = (ratePerDay: number) => RateProfile.from(steadyGains(ratePerDay / 24));

  it("keeps the start, the knots, and the end, each on the original line", () => {
    const fine = projectUsage(
      monday,
      monday + 7 * DAY,
      0,
      model({ momentum: 5 / HOUR, momentumTauMs: HOUR, profile: steady(10), isWorkDay: weekdays }),
    );
    const out = straighten(fine, localMidnights(monday, monday + 7 * DAY));
    expect(out.map((p) => p.ms)).toEqual([0, 1, 2, 3, 4, 5, 6, 7].map((d) => monday + d * DAY));
    for (const p of out) expect(p.pct).toBeCloseTo(interpolate(fine, p.ms)!);
    expect(last(out)).toEqual(last(fine));
  });

  it("with no knots is one straight run to the end", () => {
    const fine = projectUsage(monday, monday + 5 * HOUR, 20, model({ profile: steady(24) }));
    expect(straighten(fine, [])).toEqual([fine[0], last(fine)]);
  });

  it("bends at the 100% crossing and drops knots beyond it", () => {
    const fine = projectUsage(monday, monday + 7 * DAY, 80, model({ profile: steady(40) }));
    const out = straighten(fine, localMidnights(monday, monday + 7 * DAY));
    const hit = fine.find((p) => p.pct >= 100)!;
    expect(out).toEqual([fine[0], hit, last(fine)]);
  });

  it("passes short polylines through", () => {
    expect(straighten([pt(0, 5)], [1])).toEqual([pt(0, 5)]);
  });
});

describe("localMidnights", () => {
  it("lists the midnights strictly inside the range", () => {
    expect(localMidnights(monday, monday + 3 * DAY)).toEqual([monday + DAY, monday + 2 * DAY]);
    expect(localMidnights(monday + HOUR, monday + 23 * HOUR)).toEqual([]);
  });
});

describe("interpolate", () => {
  const pts = [pt(0, 0), pt(10, 20), pt(20, 20)];

  it("reads along the polyline and is null outside it", () => {
    expect(interpolate(pts, 5)).toBe(10);
    expect(interpolate(pts, 15)).toBe(20);
    expect(interpolate(pts, -1)).toBeNull();
    expect(interpolate(pts, 21)).toBeNull();
    expect(interpolate([], 0)).toBeNull();
  });
});
