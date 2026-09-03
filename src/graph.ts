// Usage-over-time graph, shown in place of the mascot: history line coloured
// by the usage thresholds, a dotted prediction at the current rate, a red bar
// at the limit, and a faint ghost of the previous window. Right-clicking it
// cycles through the limit windows the tiles show (5h, 7d, and any scoped
// model limit); hovering reads off the time and percentage under the cursor.
import {
  DEFAULT_WINDOWS,
  SESSION_ID,
  WEEKLY_ALL_ID,
  type LimitWindow,
  type UsageSnapshot,
} from "./api";
import type { UsageHistory } from "./history";
import { AMBER_AT_PCT, RED_AT_PCT } from "./thresholds";
import { trendSlope, projectUsage, type Pt } from "./trend";

const HOUR_MS = 3_600_000;
const DAY_MS = 24 * HOUR_MS;

/** How long a window runs, and how far back its rate-of-use trend looks. */
interface Span {
  windowMs: number;
  trendMs: number;
}

const SESSION_SPAN: Span = { windowMs: 5 * HOUR_MS, trendMs: 30 * 60_000 };
const WEEKLY_SPAN: Span = { windowMs: 7 * DAY_MS, trendMs: 6 * HOUR_MS };
const MONTHLY_SPAN: Span = { windowMs: 30 * DAY_MS, trendMs: DAY_MS };

/** A window's span from its id's kind (the part before any ":<model>" scope,
 *  see usage.rs). Unknown kinds are read as weekly, the shape of every scoped
 *  limit so far. */
export function windowSpan(id: string): Span {
  const kind = id.split(":")[0];
  if (kind === SESSION_ID) return SESSION_SPAN;
  if (kind.startsWith("monthly")) return MONTHLY_SPAN;
  return WEEKLY_SPAN;
}

/** The id of the window on show. Older builds stored a "session"/"weekly"
 *  mode here; "weekly" maps onto the all-model week. */
const WINDOW_KEY = "graph-mode";

const DAY_NAMES = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** What the graph reads its samples from; UsageHistory or a dev mock. */
export type GraphSource = Pick<UsageHistory, "points" | "previousWindow">;

const PAD = 8; // px at design size; rendered damped by --chrome
// Ghosts of windows that never got going are clutter, not comparison.
const GHOST_MIN_PEAK_PCT = 5;

/** A :root CSS variable's colour, so the canvas can't drift from styles.css. */
function cssColor(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

/** The :root --chrome factor (see styles.css), so the graph's furniture —
 *  padding, line widths, labels — shrinks with a small widget but stops
 *  growing past design size, keeping large graphs crisp instead of zoomed. */
function chromeScale(): number {
  const value = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--chrome"),
  );
  return Number.isFinite(value) && value > 0 ? value : 1;
}

export class UsageGraph {
  /** The chosen window id; `current()` resolves it against what's showable. */
  private windowId: string;
  private snapshot: UsageSnapshot | null = null;
  /** Window ids hidden in settings; they get no graph, as they get no tile. */
  private hidden: string[] = [];
  // Weekdays the 7-day prediction ramps, indexed Sun..Sat; all-on until set.
  private workDays: boolean[] = [true, true, true, true, true, true, true];
  /** Cursor x in canvas px while hovering, null otherwise. */
  private hoverX: number | null = null;
  /** The --chrome factor, refreshed at the top of each draw. */
  private chrome = 1;

  private readonly bg = cssColor("--bg", "#000000");
  private readonly green = cssColor("--green", "#788c5d");
  private readonly amber = cssColor("--amber", "#d97757");
  private readonly red = cssColor("--red", "#c0392b");
  private readonly dim = cssColor("--dim", "#b0aea5");
  private readonly text = cssColor("--text", "#faf9f5");
  private readonly track = cssColor("--track", "#2a2a28");

  constructor(
    private canvas: HTMLCanvasElement,
    private history: GraphSource,
  ) {
    const saved = localStorage.getItem(WINDOW_KEY);
    this.windowId = saved === "weekly" ? WEEKLY_ALL_ID : (saved ?? SESSION_ID);
    canvas.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation(); // keep the mascot picker out of graph right-clicks
      const windows = this.windows();
      const i = windows.findIndex((w) => w.id === this.current().id);
      this.windowId = windows[(i + 1) % windows.length].id;
      localStorage.setItem(WINDOW_KEY, this.windowId);
      this.draw();
    });
    // offsetX is in the element's own space, which the real-pixel layout
    // makes the same as the drawing space.
    canvas.addEventListener("mousemove", (e) => {
      this.hoverX = e.offsetX;
      this.draw();
    });
    canvas.addEventListener("mouseleave", () => {
      this.hoverX = null;
      this.draw();
    });
    new ResizeObserver(() => this.draw()).observe(canvas);
    // Keep the time axis and prediction moving between polls.
    setInterval(() => this.draw(), 60_000);
  }

  update(s: UsageSnapshot): void {
    this.snapshot = s;
    this.draw();
  }

  /** Swap the sample source (dev mock mode, or the backend log arriving). */
  setHistory(h: GraphSource): void {
    this.history = h;
    this.draw();
  }

  /** Re-render from the current source (after the backend log loads). */
  redraw(): void {
    this.draw();
  }

  /** Set which weekdays (Sun..Sat) the 7-day prediction ramps across. */
  setWorkDays(days: boolean[]): void {
    if (days.length === 7) this.workDays = days;
    this.draw();
  }

  /** Window ids the user hid in settings; the cycle skips them. */
  setHidden(ids: string[]): void {
    this.hidden = ids;
    this.draw();
  }

  /** The windows the graph can show: the snapshot's minus the hidden ones, in
   *  the API's order, mirroring the tiles. The well-known pair stands in until
   *  a poll lands (or when every window is hidden, so the graph isn't blank). */
  private windows(): readonly Pick<LimitWindow, "id" | "label">[] {
    const shown = this.snapshot?.windows.filter((w) => !this.hidden.includes(w.id)) ?? [];
    return shown.length > 0 ? shown : DEFAULT_WINDOWS;
  }

  /** The window on show: the chosen one, or the first showable once the
   *  chosen one is hidden or no longer reported. */
  private current() {
    const windows = this.windows();
    return windows.find((w) => w.id === this.windowId) ?? windows[0];
  }

  private draw(): void {
    const ctx = this.canvas.getContext("2d");
    if (!ctx) return;
    // Size the backing store to physical pixels so the canvas stays crisp,
    // then draw in its CSS-pixel space.
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width === 0 || this.canvas.clientWidth === 0) return;
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.round(rect.width * dpr);
    this.canvas.height = Math.round(rect.height * dpr);
    const s = this.canvas.width / this.canvas.clientWidth;
    ctx.setTransform(s, 0, 0, s, 0, 0);
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    ctx.clearRect(0, 0, w, h);

    this.chrome = chromeScale();
    const c = this.chrome;
    const pad = PAD * c;
    const shown = this.current();
    const span = windowSpan(shown.id);
    const now = Date.now();
    const win = this.snapshot?.windows.find((w) => w.id === shown.id) ?? null;
    // No reset time means no window is running (the last one lapsed and
    // nothing has started a new one); the axes then track the current moment.
    const resetMs = win?.resetAt ? win.resetAt * 1000 : null;
    const end = resetMs ?? now;
    const start = end - span.windowMs;
    const x = (ms: number) => pad + ((ms - start) / span.windowMs) * (w - 2 * pad);
    const y = (pct: number) => h - pad - (pct / 100) * (h - 2 * pad);

    // The corner label makes way for the hover readout (drawHover).
    if (this.hoverX === null) {
      ctx.font = `400 ${14 * c}px Grotesk, sans-serif`;
      ctx.fillStyle = this.dim;
      ctx.textBaseline = "top";
      ctx.fillText(shown.label, pad, pad + 4 * c);
    }

    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    // Faint quarter gridlines give the empty space some structure.
    ctx.strokeStyle = this.track;
    ctx.lineWidth = c;
    for (const pct of [25, 50, 75]) {
      ctx.beginPath();
      ctx.moveTo(pad, y(pct));
      ctx.lineTo(w - pad, y(pct));
      ctx.stroke();
    }

    // Ghost of the previous window, overlaid on the current axes by aligning
    // the two windows' reset times (windows have a fixed width, so their
    // starts align too). Segmenting by the reset time each sample was polled
    // with — rather than assuming windows sit back-to-back — keeps a lapsed
    // window's tail from bleeding into the start of the current one. While
    // no window is running there is no reset to align to, so the ghost sits
    // at its true time and slides into the past as the axes track "now".
    if (win) {
      const prev = this.history.previousWindow(shown.id, resetMs, span.windowMs);
      if (prev && prev.pts.some((p) => p.pct >= GHOST_MIN_PEAK_PCT)) {
        const shift = resetMs === null ? 0 : end - prev.resetMs;
        const ghost = prev.pts.filter((p) => p.ms + shift >= start);
        ctx.strokeStyle = this.dim;
        ctx.globalAlpha = 0.3;
        ctx.lineWidth = 2 * c;
        ctx.beginPath();
        for (const [i, p] of ghost.entries()) {
          const gx = x(p.ms + shift);
          if (i === 0) ctx.moveTo(gx, y(p.pct));
          else ctx.lineTo(gx, y(p.pct));
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
    }

    // The limit ceiling, and a thin marker at the reset time.
    if (resetMs !== null) {
      ctx.strokeStyle = this.dim;
      ctx.lineWidth = c;
      ctx.beginPath();
      ctx.moveTo(x(end), y(0));
      ctx.lineTo(x(end), y(100));
      ctx.stroke();
    }
    // The limit reads as a red gridline, not a frame around the panel.
    ctx.strokeStyle = this.red;
    ctx.globalAlpha = 0.7;
    ctx.lineWidth = c;
    ctx.beginPath();
    ctx.moveTo(pad, y(100));
    ctx.lineTo(w - pad, y(100));
    ctx.stroke();
    ctx.globalAlpha = 1;
    if (!win) return;

    // The line's colour follows its height: green low, blending through
    // amber at the warning threshold to red at the limit threshold.
    const gradient = ctx.createLinearGradient(0, y(0), 0, y(100));
    gradient.addColorStop(0, this.green);
    gradient.addColorStop(AMBER_AT_PCT / 100, this.amber);
    gradient.addColorStop(RED_AT_PCT / 100, this.red);

    const pts = this.history.points(shown.id, start, resetMs).filter((p) => p.ms <= now);
    pts.push({ ms: Math.min(now, end), pct: win.utilization });

    // Soft area fill anchors the line to the baseline.
    if (pts.length >= 2) {
      ctx.save();
      ctx.globalAlpha = 0.12;
      ctx.fillStyle = gradient;
      ctx.beginPath();
      pts.forEach((p, i) =>
        i === 0 ? ctx.moveTo(x(p.ms), y(p.pct)) : ctx.lineTo(x(p.ms), y(p.pct)),
      );
      ctx.lineTo(x(pts[pts.length - 1].ms), y(0));
      ctx.lineTo(x(pts[0].ms), y(0));
      ctx.closePath();
      ctx.fill();
      ctx.restore();
    }

    ctx.lineWidth = 3 * c;
    this.strokePolyline(ctx, pts, gradient, x, y);

    let proj: Pt[] | null = null;
    const slope = trendSlope(pts, span.trendMs, now);
    if (win.resetAt && slope !== null && now < end) {
      const rise = Math.max(0, slope);
      // The work-day mask only shapes multi-day windows; the 5h projection is
      // intra-day, so it always ramps.
      const isWorkDay =
        span.windowMs > DAY_MS
          ? (ms: number) => this.workDays[new Date(ms).getDay()] !== false
          : () => true;
      proj = projectUsage(now, end, win.utilization, rise, isWorkDay);
      ctx.setLineDash([c, 6 * c]);
      this.strokePolyline(ctx, proj, gradient, x, y);
      ctx.setLineDash([]);
    }

    // A bright "now" marker at the end of the live line.
    const cur = pts[pts.length - 1];
    ctx.fillStyle = this.text;
    ctx.beginPath();
    ctx.arc(x(cur.ms), y(cur.pct), 2.5 * c, 0, Math.PI * 2);
    ctx.fill();

    this.drawHover(ctx, w, start, span.windowMs, pts, proj, y);
  }

  /** Crosshair plus a time · percentage readout for the hovered instant. */
  private drawHover(
    ctx: CanvasRenderingContext2D,
    w: number,
    start: number,
    windowMs: number,
    pts: Pt[],
    proj: Pt[] | null,
    y: (pct: number) => number,
  ): void {
    if (this.hoverX === null) return;
    const c = this.chrome;
    const pad = PAD * c;
    const hx = Math.min(Math.max(this.hoverX, pad), w - pad);
    const t = start + ((hx - pad) / (w - 2 * pad)) * windowMs;

    ctx.strokeStyle = this.dim;
    ctx.globalAlpha = 0.5;
    ctx.lineWidth = c;
    ctx.beginPath();
    ctx.moveTo(hx, y(0));
    ctx.lineTo(hx, y(100));
    ctx.stroke();
    ctx.globalAlpha = 1;

    // Read the recorded line first; past its end, fall back to the prediction.
    const recorded = interpolate(pts, t);
    const pct = recorded ?? (proj ? interpolate(proj, t) : null);
    if (pct !== null) {
      ctx.fillStyle = this.text;
      ctx.beginPath();
      ctx.arc(hx, y(pct), 2 * c, 0, Math.PI * 2);
      ctx.fill();
    }

    // The readout takes the corner label's spot (the label is hidden while
    // hovering), on a dark chip so it stays legible over the lines.
    const when = new Date(t);
    const hh = String(when.getHours()).padStart(2, "0");
    const mm = String(when.getMinutes()).padStart(2, "0");
    // Multi-day windows need the weekday to place the time.
    const day = windowMs > DAY_MS ? `${DAY_NAMES[when.getDay()]} ` : "";
    const value = pct === null ? "" : ` · ${pct.toFixed(0)}%${recorded === null ? " est" : ""}`;
    const label = `${day}${hh}:${mm}${value}`;
    ctx.font = `400 ${14 * c}px Grotesk, sans-serif`;
    ctx.textBaseline = "top";
    const tw = ctx.measureText(label).width;
    ctx.globalAlpha = 0.8;
    ctx.fillStyle = this.bg;
    ctx.beginPath();
    if (typeof ctx.roundRect === "function")
      ctx.roundRect(pad - 3 * c, pad, tw + 8 * c, 21 * c, 5 * c);
    else ctx.rect(pad - 3 * c, pad, tw + 8 * c, 21 * c);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.fillStyle = this.text;
    ctx.fillText(label, pad + c, pad + 4 * c);
  }

  private strokePolyline(
    ctx: CanvasRenderingContext2D,
    pts: Pt[],
    style: CanvasGradient,
    x: (ms: number) => number,
    y: (pct: number) => number,
  ): void {
    if (pts.length === 1) {
      // A lone sample has no line to stroke; mark it with a dot.
      ctx.fillStyle = style;
      ctx.beginPath();
      ctx.arc(x(pts[0].ms), y(pts[0].pct), 2 * this.chrome, 0, Math.PI * 2);
      ctx.fill();
      return;
    }
    ctx.strokeStyle = style;
    ctx.beginPath();
    pts.forEach((p, i) =>
      i === 0 ? ctx.moveTo(x(p.ms), y(p.pct)) : ctx.lineTo(x(p.ms), y(p.pct)),
    );
    ctx.stroke();
  }
}

/** Linear interpolation along a polyline, null outside its time range. */
function interpolate(series: Pt[], t: number): number | null {
  if (series.length === 0 || t < series[0].ms || t > series[series.length - 1].ms) return null;
  for (let i = 1; i < series.length; i++) {
    if (t <= series[i].ms) {
      const a = series[i - 1];
      const b = series[i];
      const f = b.ms === a.ms ? 0 : (t - a.ms) / (b.ms - a.ms);
      return a.pct + (b.pct - a.pct) * f;
    }
  }
  return series[series.length - 1].pct;
}
