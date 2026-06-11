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
