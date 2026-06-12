// Usage-over-time graph, shown in place of the mascot: history line coloured
// by the usage thresholds, a dotted prediction at the current rate, a red bar
// at the limit, and a faint ghost of the previous window. Right-clicking it
// switches between the 5-hour and 7-day windows.
import type { UsageSnapshot } from "./api";
import type { UsageHistory } from "./history";
import { trendSlope, projectUsage, type Pt } from "./trend";

type Mode = "session" | "weekly";

const MODE = {
  session: { key: "five", windowMs: 5 * 3_600_000, trendMs: 30 * 60_000, label: "5h" },
  weekly: { key: "week", windowMs: 7 * 86_400_000, trendMs: 6 * 3_600_000, label: "7d" },
} as const;

const MODE_KEY = "graph-mode";

// Canvas can't resolve CSS variables; these mirror :root in styles.css.
const GREEN = "#788c5d";
const AMBER = "#d97757";
const RED = "#c0392b";
const DIM = "#b0aea5";
const TEXT = "#faf9f5";
const TRACK = "#2a2a28";

/** What the graph reads its samples from; UsageHistory or a dev mock. */
export type GraphSource = Pick<UsageHistory, "points">;

const PAD = 8; // design px

export class UsageGraph {
  private mode: Mode;
  private snapshot: UsageSnapshot | null = null;
  // Weekdays the 7-day prediction ramps, indexed Sun..Sat; all-on until set.
  private workDays: boolean[] = [true, true, true, true, true, true, true];

  constructor(
    private canvas: HTMLCanvasElement,
    private history: GraphSource,
  ) {
    this.mode = localStorage.getItem(MODE_KEY) === "weekly" ? "weekly" : "session";
    canvas.title = "Right-click: switch 5h / 7d";
    canvas.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation(); // keep the mascot picker out of graph right-clicks
      this.mode = this.mode === "session" ? "weekly" : "session";
      localStorage.setItem(MODE_KEY, this.mode);
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

  /** Swap the sample source (dev mock mode). */
  setHistory(h: GraphSource): void {
    this.history = h;
    this.draw();
  }

  /** Set which weekdays (Sun..Sat) the 7-day prediction ramps across. */
  setWorkDays(days: boolean[]): void {
    if (days.length === 7) this.workDays = days;
    this.draw();
  }

  private draw(): void {
    const ctx = this.canvas.getContext("2d");
    if (!ctx) return;
    // Size the backing store to on-screen pixels so the (scaled-down)
    // design-space canvas stays crisp, then draw in design units.
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

    const cfg = MODE[this.mode];
    const now = Date.now();
    const win = this.mode === "session" ? this.snapshot?.fiveHour : this.snapshot?.sevenDay;
    const end = win?.resetAt ? win.resetAt * 1000 : now;
    const start = end - cfg.windowMs;
    const x = (ms: number) => PAD + ((ms - start) / cfg.windowMs) * (w - 2 * PAD);
    const y = (pct: number) => h - PAD - (pct / 100) * (h - 2 * PAD);

    ctx.font = "400 14px Grotesk, sans-serif";
    ctx.fillStyle = DIM;
    ctx.textBaseline = "top";
    ctx.fillText(cfg.label, PAD, PAD + 4);

    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    // Faint quarter gridlines give the empty space some structure.
    ctx.strokeStyle = TRACK;
    ctx.lineWidth = 1;
    for (const pct of [25, 50, 75]) {
      ctx.beginPath();
      ctx.moveTo(PAD, y(pct));
      ctx.lineTo(w - PAD, y(pct));
      ctx.stroke();
    }

    // Ghost of the previous window, time-shifted onto the same axes.
    // Walk back up to 4 windows to find one with data (handles skipped windows).
    let ghost: Pt[] = [];
    let ghostShift = 0;
    for (let n = 1; n <= 4; n++) {
      const gStart = start - n * cfg.windowMs;
      const candidates = this.history
        .points(cfg.key, gStart)
        .filter((p) => p.ms < gStart + cfg.windowMs);
      if (candidates.length >= 2) {
        ghost = candidates;
        ghostShift = n * cfg.windowMs;
        break;
      }
    }
    if (ghost.length >= 2) {
      ctx.strokeStyle = DIM;
      ctx.globalAlpha = 0.3;
      ctx.lineWidth = 2;
      ctx.beginPath();
      for (const [i, p] of ghost.entries()) {
        const gx = x(p.ms + ghostShift);
        if (i === 0) ctx.moveTo(gx, y(p.pct));
        else ctx.lineTo(gx, y(p.pct));
      }
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // The limit ceiling, and a thin marker at the reset time.
    if (win?.resetAt) {
      ctx.strokeStyle = DIM;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x(end), y(0));
      ctx.lineTo(x(end), y(100));
      ctx.stroke();
    }
    // The limit reads as a red gridline, not a frame around the panel.
    ctx.strokeStyle = RED;
    ctx.globalAlpha = 0.7;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(PAD, y(100));
    ctx.lineTo(w - PAD, y(100));
    ctx.stroke();
    ctx.globalAlpha = 1;
    if (!win) return;

    // The line's colour follows its height: green low, blending through
    // amber around the warning threshold to red at the limit.
    const gradient = ctx.createLinearGradient(0, y(0), 0, y(100));
    gradient.addColorStop(0, GREEN);
    gradient.addColorStop(0.5, AMBER);
    gradient.addColorStop(0.85, RED);

    const pts = this.history.points(cfg.key, start).filter((p) => p.ms <= now);
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

    ctx.lineWidth = 3;
    this.strokePolyline(ctx, pts, gradient, x, y);

    const slope = trendSlope(pts, cfg.trendMs, now);
    if (win.resetAt && slope !== null && now < end) {
      const rise = Math.max(0, slope);
      // The work-day mask only shapes the weekly window; the 5h projection is
      // intra-day, so it always ramps.
      const isWorkDay =
        this.mode === "weekly"
          ? (ms: number) => this.workDays[new Date(ms).getDay()] !== false
          : () => true;
      const proj = projectUsage(now, end, win.utilization, rise, isWorkDay);
      ctx.setLineDash([1, 6]);
      this.strokePolyline(ctx, proj, gradient, x, y);
      ctx.setLineDash([]);
    }

    // A bright "now" marker at the end of the live line.
    const cur = pts[pts.length - 1];
    ctx.fillStyle = TEXT;
    ctx.beginPath();
    ctx.arc(x(cur.ms), y(cur.pct), 2.5, 0, Math.PI * 2);
    ctx.fill();
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
      ctx.arc(x(pts[0].ms), y(pts[0].pct), 2, 0, Math.PI * 2);
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
