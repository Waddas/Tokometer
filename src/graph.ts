// Usage-over-time graph, shown in place of the mascot: history line coloured
// by the usage thresholds, a dotted prediction at the current rate, a red bar
// at the limit, and a faint ghost of the previous window. Right-clicking it
// switches between the 5-hour and 7-day windows.
import type { UsageSnapshot } from "./api";
import type { UsageHistory } from "./history";
import { colorSegments, trendSlope, type Pt, type Run } from "./trend";

type Mode = "session" | "weekly";

const MODE = {
  session: { key: "five", windowMs: 5 * 3_600_000, trendMs: 30 * 60_000, label: "5h" },
  weekly: { key: "week", windowMs: 7 * 86_400_000, trendMs: 6 * 3_600_000, label: "7d" },
} as const;

const MODE_KEY = "graph-mode";

// Canvas can't resolve CSS variables; these mirror :root in styles.css.
const BAND_COLORS = ["#788c5d", "#d97757", "#c0392b"];
const RED = "#c0392b";
const DIM = "#b0aea5";

const PAD = 8; // design px

export class UsageGraph {
  private mode: Mode;
  private snapshot: UsageSnapshot | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    private history: UsageHistory,
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
    ctx.fillText(cfg.label, PAD, PAD + 6); // sits below the ceiling line

    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    // Ghost of the previous window, time-shifted onto the same axes.
    const ghost = this.history
      .points(cfg.key, start - cfg.windowMs)
      .filter((p) => p.ms < start);
    if (ghost.length >= 2) {
      ctx.strokeStyle = DIM;
      ctx.globalAlpha = 0.3;
      ctx.lineWidth = 2;
      ctx.beginPath();
      for (const [i, p] of ghost.entries()) {
        const gx = x(p.ms + cfg.windowMs);
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
    ctx.strokeStyle = RED;
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(PAD, y(100));
    ctx.lineTo(w - PAD, y(100));
    ctx.stroke();
    if (!win) return;

    const pts = this.history.points(cfg.key, start).filter((p) => p.ms <= now);
    pts.push({ ms: Math.min(now, end), pct: win.utilization });

    ctx.lineWidth = 3;
    this.strokeRuns(ctx, colorSegments(pts), x, y);

    const slope = trendSlope(pts, cfg.trendMs, now);
    if (win.resetAt && slope !== null && now < end) {
      const rise = Math.max(0, slope);
      const cur = win.utilization;
      const proj: Pt[] = [{ ms: now, pct: cur }];
      const fullAt = rise > 0 ? now + (100 - cur) / rise : Infinity;
      if (fullAt < end) proj.push({ ms: fullAt, pct: 100 }, { ms: end, pct: 100 });
      else proj.push({ ms: end, pct: cur + rise * (end - now) });
      ctx.setLineDash([1, 6]);
      this.strokeRuns(ctx, colorSegments(proj), x, y);
      ctx.setLineDash([]);
    }
  }

  private strokeRuns(
    ctx: CanvasRenderingContext2D,
    runs: Run[],
    x: (ms: number) => number,
    y: (pct: number) => number,
  ): void {
    for (const run of runs) {
      ctx.strokeStyle = BAND_COLORS[run.band];
      if (run.pts.length === 1 && runs.length === 1) {
        // A lone sample has no line to stroke; mark it with a dot.
        ctx.fillStyle = BAND_COLORS[run.band];
        ctx.beginPath();
        ctx.arc(x(run.pts[0].ms), y(run.pts[0].pct), 2, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      ctx.beginPath();
      run.pts.forEach((p, i) =>
        i === 0 ? ctx.moveTo(x(p.ms), y(p.pct)) : ctx.lineTo(x(p.ms), y(p.pct)),
      );
      ctx.stroke();
    }
  }
}
