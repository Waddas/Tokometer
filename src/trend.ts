// Pure geometry for the usage graph: threshold-band colouring and the
// usage-rate slope behind the prediction line.

export interface Pt {
  /** unix epoch ms */
  ms: number;
  /** 0-100 percent */
  pct: number;
}

/** Band edges; match the pctColor thresholds in usage.ts. */
const EDGES = [50, 80];

/** 0 = green, 1 = amber, 2 = red. */
export function band(pct: number): number {
  if (pct >= 80) return 2;
  if (pct >= 50) return 1;
  return 0;
}

export interface Run {
  band: number;
  pts: Pt[];
}

/**
 * Split a polyline into runs of constant colour band, inserting interpolated
 * points at threshold crossings. Adjacent runs share their boundary point so
 * the strokes connect.
 */
export function colorSegments(pts: Pt[]): Run[] {
  if (pts.length === 0) return [];
  const runs: Run[] = [];
  let cur: Run = { band: band(pts[0].pct), pts: [pts[0]] };
  for (let i = 1; i < pts.length; i++) {
    let from = pts[i - 1];
    const to = pts[i];
    while (band(to.pct) !== cur.band) {
      const up = band(to.pct) > cur.band;
      const edge = up ? EDGES[cur.band] : EDGES[cur.band - 1];
      const t = (edge - from.pct) / (to.pct - from.pct);
      const crossing: Pt = { ms: from.ms + t * (to.ms - from.ms), pct: edge };
      cur.pts.push(crossing);
      runs.push(cur);
      cur = { band: cur.band + (up ? 1 : -1), pts: [crossing] };
      from = crossing;
    }
    cur.pts.push(to);
  }
  runs.push(cur);
  return runs;
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
