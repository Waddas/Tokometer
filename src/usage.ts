// Usage screen renderer — replicates Clawdmeter's firmware ui.cpp usage view:
// threshold-colored percentages/bars, reset countdowns, and the spinner +
// whimsical-verb status line.
import type { UsageSnapshot, UsageWindow } from "./api";

const SPINNER = ["·", "✻", "✽", "✶", "✳", "✢"];
const SPINNER_MS = [260, 130, 130, 130, 130, 260];
const SPINNER_PHASES = 2 * (SPINNER.length - 1); // ping-pong 0..5..0
const MSG_MS = 4000;
const CONNECT_DWELL_MS = 5000;

// anim_messages from ui.cpp, verbatim.
const MESSAGES = [
  "Accomplishing", "Elucidating", "Perusing",
  "Actioning", "Enchanting", "Philosophising",
  "Actualizing", "Envisioning", "Pondering",
  "Baking", "Finagling", "Pontificating",
  "Booping", "Flibbertigibbeting", "Processing",
  "Brewing", "Forging", "Puttering",
  "Calculating", "Forming", "Puzzling",
  "Cerebrating", "Frolicking", "Reticulating",
  "Channelling", "Generating", "Ruminating",
  "Churning", "Germinating", "Scheming",
  "Clauding", "Hatching", "Schlepping",
  "Coalescing", "Herding", "Shimmying",
  "Cogitating", "Honking", "Shucking",
  "Combobulating", "Hustling", "Simmering",
  "Computing", "Ideating", "Smooshing",
  "Concocting", "Imagining", "Spelunking",
  "Conjuring", "Incubating", "Spinning",
  "Considering", "Inferring", "Stewing",
  "Contemplating", "Jiving", "Sussing",
  "Cooking", "Manifesting", "Synthesizing",
  "Crafting", "Marinating", "Thinking",
  "Creating", "Meandering", "Tinkering",
  "Crunching", "Moseying", "Transmuting",
  "Deciphering", "Mulling", "Unfurling",
  "Deliberating", "Mustering", "Unravelling",
  "Determining", "Musing", "Vibing",
  "Discombobulating", "Noodling", "Wandering",
  "Divining", "Percolating", "Whirring",
  "Doing", "Wibbling",
  "Effecting", "Wizarding",
  "Working", "Wrangling",
];

function pctColor(pct: number): string {
  if (pct >= 80) return "var(--red)";
  if (pct >= 50) return "var(--amber)";
  return "var(--green)";
}

// format_reset_time from ui.cpp; compact mode drops the "Resets in" prefix.
function formatReset(mins: number, compact: boolean): string {
  if (mins < 0) return "---";
  let t: string;
  if (mins < 60) t = `${mins}m`;
  else if (mins < 1440) t = `${Math.floor(mins / 60)}h ${mins % 60}m`;
  else t = `${Math.floor(mins / 1440)}d ${Math.floor((mins % 1440) / 60)}h`;
  return compact ? t : `Resets in ${t}`;
}

interface PanelEls {
  pct: HTMLElement;
  fill: HTMLElement;
  reset: HTMLElement;
}

export class UsageRenderer {
  private panels: Record<"session" | "weekly", PanelEls>;
  private spinnerEl: HTMLElement;
  private msgEl: HTMLElement;
  private snapshot: UsageSnapshot | null = null;
  private compact = false;
  private okSince = 0;
  private spinnerPhase = 0;
  private message = MESSAGES[Math.floor(Math.random() * MESSAGES.length)];

  constructor(root: HTMLElement) {
    const panel = (name: string): PanelEls => {
      const el = root.querySelector(`.panel[data-window="${name}"]`)!;
      return {
        pct: el.querySelector(".pct")!,
        fill: el.querySelector(".fill")!,
        reset: el.querySelector(".reset")!,
      };
    };
    this.panels = { session: panel("session"), weekly: panel("weekly") };
    this.spinnerEl = root.querySelector("#spinner")!;
    this.msgEl = root.querySelector("#status-msg")!;

    this.tickSpinner();
    setInterval(() => this.rotateMessage(), MSG_MS);
    // Keep reset countdowns fresh between polls.
    setInterval(() => this.renderResets(), 30_000);
  }

  setCompact(compact: boolean): void {
    this.compact = compact;
    this.renderResets();
  }

  update(s: UsageSnapshot): void {
    this.snapshot = s;
    if (s.status === "ok" && this.okSince === 0) this.okSince = Date.now();
    if (s.status !== "ok") this.okSince = 0;

    this.renderPanel(this.panels.session, s.fiveHour);
    this.renderPanel(this.panels.weekly, s.sevenDay);
    this.renderStatusText();
  }

  private renderPanel(els: PanelEls, w: UsageWindow | null): void {
    if (!w) {
      els.pct.textContent = "--%";
      els.pct.style.color = "var(--dim)";
      els.fill.style.width = "0%";
      els.reset.textContent = "---";
      return;
    }
    const pct = Math.round(w.utilization);
    els.pct.textContent = `${pct}%`;
    els.pct.style.color = pctColor(pct);
    els.fill.style.width = `${Math.min(100, Math.max(0, w.utilization))}%`;
    els.fill.style.background = pctColor(pct);
    els.reset.textContent = this.resetText(w);
  }

  private resetText(w: UsageWindow): string {
    if (w.resetAt === null) return "---";
    const mins = Math.max(0, Math.round((w.resetAt * 1000 - Date.now()) / 60_000));
    return formatReset(mins, this.compact);
  }

  private renderResets(): void {
    const s = this.snapshot;
    if (!s) return;
    if (s.fiveHour) this.panels.session.reset.textContent = this.resetText(s.fiveHour);
    if (s.sevenDay) this.panels.weekly.reset.textContent = this.resetText(s.sevenDay);
  }

  private renderStatusText(): void {
    const s = this.snapshot;
    let text: string;
    if (!s) {
      text = "Connecting…";
    } else if (s.status === "error") {
      text = s.error ?? "error";
    } else if (Date.now() - this.okSince < CONNECT_DWELL_MS) {
      text = "Connected";
    } else {
      text = `${this.message}…`;
    }
    this.msgEl.textContent = text;
  }

  private rotateMessage(): void {
    this.message = MESSAGES[Math.floor(Math.random() * MESSAGES.length)];
    this.renderStatusText();
  }

  private tickSpinner(): void {
    const idx =
      this.spinnerPhase < SPINNER.length
        ? this.spinnerPhase
        : SPINNER_PHASES - this.spinnerPhase;
    this.spinnerEl.textContent = SPINNER[idx];
    this.spinnerPhase = (this.spinnerPhase + 1) % SPINNER_PHASES;
    setTimeout(() => this.tickSpinner(), SPINNER_MS[idx]);
  }
}
