// Pure forecasting for the usage graph. Two signals feed the prediction line:
// the momentum of the last few polls, and a profile of how fast this user
// usually burns usage at each hour of the week. Momentum dominates the next
// stretch and the profile the rest, so a heavy session shows up immediately
// while nights and weekends still flatten the line.

export interface Pt {
  /** unix epoch ms */
  ms: number;
  /** 0-100 percent */
  pct: number;
}

/**
 * Usage rate in percent per ms over the points within `spanMs` of `now`,
 * or null until the points cover at least an eighth of that span. A
 * least-squares fit over every recent point, so one noisy sample at either
 * end can't swing the prediction the way an endpoint slope would.
 */
export function trendSlope(pts: Pt[], spanMs: number, now: number): number | null {
  const recent = pts.filter((p) => now - p.ms <= spanMs);
  if (recent.length < 2) return null;
  const first = recent[0];
  const last = recent[recent.length - 1];
  if (last.ms - first.ms < spanMs / 8) return null;
  let st = 0;
  let sp = 0;
  let stt = 0;
  let stp = 0;
  for (const p of recent) {
    const t = p.ms - first.ms; // offset to keep t*t well inside float precision
    st += t;
    sp += p.pct;
    stt += t * t;
    stp += t * p.pct;
  }
  const n = recent.length;
  const denom = n * stt - st * st;
  if (denom === 0) return null;
  return (n * stp - st * sp) / denom;
}

/** Usage gained between two consecutive polls of one window. */
export interface Gain {
  fromMs: number;
  toMs: number;
  /** percent; negative values (jitter, a lapsed window) count as none */
  pct: number;
}

const HOUR_MS = 3_600_000;
const HOURS_PER_WEEK = 7 * 24;
// A bucket needs this much observed time before its own average outweighs
// the estimate it is shrunk toward.
const PRIOR_MS = 3 * HOUR_MS;
// Past this, a gap is the app being closed; where the usage landed is unknown.
const MAX_GAP_MS = 24 * HOUR_MS;

/** Observed usage over observed time, for one slice of the week. */
class Bin {
  gain = 0;
  ms = 0;
  add(gain: number, ms: number): void {
    this.gain += gain;
    this.ms += ms;
  }
  /** Average rate, shrunk toward `prior` until enough time has been observed. */
  rate(prior: number): number {
    return (this.gain + PRIOR_MS * prior) / (this.ms + PRIOR_MS);
  }
}

const bins = (n: number) => Array.from({ length: n }, () => new Bin());

/**
 * Average usage rate for each local hour of the week, learned from the gains
 * between consecutive polls. Each gain is spread over the hours it spans, so
 * the sampling cadence doesn't matter. A sparsely observed hour is shrunk
 * toward what its day and its hour of day each suggest (day effect × hour
 * effect), so a quiet Saturday noon isn't inflated by busy weekday noons and
 * the profile is usable after a day yet sharpens as weeks accumulate.
 */
export class RateProfile {
  private readonly hourOfWeek = bins(HOURS_PER_WEEK);
  private readonly dayOfWeek = bins(7);
  private readonly hourOfDay = bins(24);
  private readonly all = new Bin();

  static from(gains: Iterable<Gain>): RateProfile {
    const profile = new RateProfile();
    for (const g of gains) profile.add(g);
    return profile;
  }

  /** True once at least one gain has been recorded. */
  get hasData(): boolean {
    return this.all.ms > 0;
  }

  add(g: Gain): void {
    const span = g.toMs - g.fromMs;
    if (span <= 0 || span > MAX_GAP_MS) return;
    const rate = Math.max(0, g.pct) / span;
    let t = g.fromMs;
    while (t < g.toMs) {
      const next = Math.min(g.toMs, nextLocalHour(t));
      const ms = next - t;
      const d = new Date(t);
      this.hourOfWeek[d.getDay() * 24 + d.getHours()].add(rate * ms, ms);
      this.dayOfWeek[d.getDay()].add(rate * ms, ms);
      this.hourOfDay[d.getHours()].add(rate * ms, ms);
      this.all.add(rate * ms, ms);
      t = next;
    }
  }

  /** Expected usage rate at `ms`, in percent per ms; 0 without data. */
  rateAt(ms: number): number {
    if (!this.hasData || this.all.gain === 0) return 0;
    const d = new Date(ms);
    const overall = this.all.gain / this.all.ms;
    const day = this.dayOfWeek[d.getDay()].rate(overall);
    const hour = this.hourOfDay[d.getHours()].rate(overall);
    return this.hourOfWeek[d.getDay() * 24 + d.getHours()].rate((day * hour) / overall);
  }
}

export interface ForecastModel {
  /** Recent usage rate in percent per ms, or null when too thin to fit. */
  momentum: number | null;
  /** Time constant over which the forecast lets go of momentum for the
   *  profile; Infinity keeps the momentum as a constant rate throughout. */
  momentumTauMs: number;
  profile: RateProfile;
  /** Days the forecast may ramp on; others hold flat. Called with a ms timestamp. */
  isWorkDay: (ms: number) => boolean;
}

/**
 * Prediction polyline from `now`/`cur` to `end`. The rate at each instant is
 * momentum blended with the profile, the momentum share decaying as
 * exp(-(t - now) / tau), integrated exactly over each segment. Segments break
 * at local hour boundaries (where the profile changes) and every half tau
 * (so the decay reads as a curve). Non-work days contribute nothing. Caps at
 * 100% and stops there.
 */
export function projectUsage(now: number, end: number, cur: number, model: ForecastModel): Pt[] {
  const proj: Pt[] = [{ ms: now, pct: cur }];
  if (end <= now || cur >= 100) {
    proj.push({ ms: end, pct: Math.min(100, cur) });
    return proj;
  }
  const { momentumTauMs: tau, profile, isWorkDay } = model;
  const momentum = model.momentum === null ? null : Math.max(0, model.momentum);
  const maxStep = Number.isFinite(tau) ? Math.max(60_000, tau / 2) : Infinity;
  let t = now;
  let pct = cur;
  while (t < end) {
    const next = Math.min(end, nextLocalHour(t), t + maxStep);
    if (isWorkDay(t)) {
      const span = next - t;
      let gain: number;
      if (momentum === null) {
        gain = profile.rateAt(t) * span;
      } else {
        // ∫ exp(-(s - now) / tau) ds over [t, next]; the whole span if tau is infinite
        const momentumMs = Number.isFinite(tau)
          ? tau * (Math.exp(-(t - now) / tau) - Math.exp(-(next - now) / tau))
          : span;
        gain = momentum * momentumMs + profile.rateAt(t) * (span - momentumMs);
      }
      if (pct + gain >= 100) {
        proj.push({ ms: t + (span * (100 - pct)) / gain, pct: 100 }, { ms: end, pct: 100 });
        return proj;
      }
      pct += gain;
    }
    proj.push({ ms: next, pct });
    t = next;
  }
  return proj;
}

/**
 * Reduce a projection to the vertices a reader needs: its start, the given
 * knots (say, local midnights, where the work-day mask changes the slope),
 * the moment it reaches 100%, and its end. Every kept vertex sits exactly on
 * the original line, so the endpoint and the limit crossing are unchanged;
 * only the smooth intermediate shape, an expected value that never looks
 * like real bursty usage, is dropped in favour of straight segments.
 */
export function straighten(proj: Pt[], knots: number[]): Pt[] {
  if (proj.length < 2) return proj;
  const start = proj[0];
  const end = proj[proj.length - 1];
  const hit = start.pct < 100 ? proj.find((p) => p.pct >= 100) : undefined;
  const cutoff = hit ? hit.ms : end.ms;
  const out: Pt[] = [start];
  for (const k of [...knots].sort((a, b) => a - b)) {
    if (k <= start.ms || k >= cutoff) continue;
    out.push({ ms: k, pct: interpolate(proj, k)! });
  }
  if (hit) out.push(hit);
  if (out[out.length - 1].ms < end.ms) out.push(end);
  return out;
}

/** Local midnights strictly between `from` and `to`. */
export function localMidnights(from: number, to: number): number[] {
  const out: number[] = [];
  for (let t = nextLocalMidnight(from); t < to; t = nextLocalMidnight(t)) out.push(t);
  return out;
}

/** Linear interpolation along a polyline, null outside its time range. */
export function interpolate(pts: Pt[], t: number): number | null {
  if (pts.length === 0 || t < pts[0].ms || t > pts[pts.length - 1].ms) return null;
  for (let i = 1; i < pts.length; i++) {
    if (t <= pts[i].ms) {
      const a = pts[i - 1];
      const b = pts[i];
      const f = b.ms === a.ms ? 0 : (t - a.ms) / (b.ms - a.ms);
      return a.pct + (b.pct - a.pct) * f;
    }
  }
  return pts[pts.length - 1].pct;
}

/** First local midnight strictly after `ms`. */
function nextLocalMidnight(ms: number): number {
  const d = new Date(ms);
  d.setHours(24, 0, 0, 0);
  const next = d.getTime();
  return next > ms ? next : ms + 24 * HOUR_MS;
}

/** First local hour boundary strictly after `ms`. */
function nextLocalHour(ms: number): number {
  const d = new Date(ms);
  d.setMinutes(60, 0, 0);
  const next = d.getTime();
  // Guard against a DST fold making the boundary not advance.
  return next > ms ? next : ms + HOUR_MS;
}
