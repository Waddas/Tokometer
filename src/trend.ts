// Pure geometry for the usage graph: the usage-rate slope behind the
// prediction line.

export interface Pt {
  /** unix epoch ms */
  ms: number;
  /** 0-100 percent */
  pct: number;
}

/**
 * Usage rate in percent per ms over the points within `spanMs` of `now`,
 * or null until the points cover at least an eighth of that span.
 */
export function trendSlope(pts: Pt[], spanMs: number, now: number): number | null {
  const recent = pts.filter((p) => now - p.ms <= spanMs);
  if (recent.length < 2) return null;
  const first = recent[0];
  const last = recent[recent.length - 1];
  if (last.ms - first.ms < spanMs / 8) return null;
  return (last.pct - first.pct) / (last.ms - first.ms);
}

/**
 * Prediction polyline from `now`/`cur` to `end`, rising at `rise` (pct per ms,
 * assumed >= 0) only across days `isWorkDay` accepts and holding flat on the
 * rest. Caps at 100%, stopping once reached. `isWorkDay` is called with a ms
 * timestamp; the caller decides how to map it to a day (local vs UTC).
 *
 * With every day a work day this is the same straight ramp as a constant-rate
 * projection; unchecked days flatten their span so the line doesn't extrapolate
 * usage across time the user won't be working.
 */
export function projectUsage(
  now: number,
  end: number,
  cur: number,
  rise: number,
  isWorkDay: (ms: number) => boolean,
): Pt[] {
  const proj: Pt[] = [{ ms: now, pct: cur }];
  if (rise <= 0 || cur >= 100 || end <= now) {
    proj.push({ ms: end, pct: Math.min(100, cur) });
    return proj;
  }
  let t = now;
  let pct = cur;
  // Walk local-midnight to local-midnight so each segment sits in one day.
  while (t < end) {
    const next = Math.min(end, nextLocalMidnight(t));
    if (isWorkDay(t)) {
      const span = next - t;
      const reach = pct + rise * span;
      if (reach >= 100) {
        proj.push({ ms: t + (100 - pct) / rise, pct: 100 }, { ms: end, pct: 100 });
        return proj;
      }
      pct = reach;
    }
    proj.push({ ms: next, pct });
    t = next;
  }
  return proj;
}

/** First local-midnight strictly after `ms`. */
function nextLocalMidnight(ms: number): number {
  const d = new Date(ms);
  d.setHours(24, 0, 0, 0);
  return d.getTime();
}
