// Usage tile renderer — threshold-coloured percentages and reset countdowns,
// thresholds and time format from Clawdmeter's firmware ui.cpp.
import type { UsageSnapshot, UsageWindow } from "./api";
import { AMBER_AT_PCT, RED_AT_PCT } from "./thresholds";

function pctColor(pct: number): string {
  if (pct >= RED_AT_PCT) return "var(--red)";
  if (pct >= AMBER_AT_PCT) return "var(--amber)";
  return "var(--green)";
}

// format_reset_time from ui.cpp, without the "Resets in" prefix.
function formatReset(mins: number): string {
  if (mins < 0) return "---";
  if (mins < 60) return `${mins}m`;
  if (mins < 1440) return `${Math.floor(mins / 60)}h ${mins % 60}m`;
  return `${Math.floor(mins / 1440)}d ${Math.floor((mins % 1440) / 60)}h`;
}

interface PanelEls {
  pct: HTMLElement;
  reset: HTMLElement;
}

export class UsageRenderer {
  private panels: Record<"session" | "weekly", PanelEls>;
  private snapshot: UsageSnapshot | null = null;

  constructor(root: HTMLElement) {
    const panel = (name: string): PanelEls => {
      const el = root.querySelector(`.panel[data-window="${name}"]`)!;
      return {
        pct: el.querySelector(".pct")!,
        reset: el.querySelector(".reset")!,
      };
    };
    this.panels = { session: panel("session"), weekly: panel("weekly") };
    // Keep reset countdowns fresh between polls.
    setInterval(() => this.renderResets(), 30_000);
  }

  /** Render a snapshot; `stale` drains the threshold colours to grey, for
   *  showing the last known values while polling fails. */
  update(s: UsageSnapshot, stale = false): void {
    this.snapshot = s;
    this.renderPanel(this.panels.session, s.fiveHour, stale);
    this.renderPanel(this.panels.weekly, s.sevenDay, stale);
  }

  private renderPanel(els: PanelEls, w: UsageWindow | null, stale: boolean): void {
    if (!w) {
      els.pct.textContent = "--%";
      els.pct.style.color = "var(--dim)";
      els.reset.textContent = "---";
      return;
    }
    const pct = Math.round(w.utilization);
    els.pct.textContent = `${pct}%`;
    els.pct.style.color = stale ? "var(--dim)" : pctColor(pct);
    els.reset.textContent = this.resetText(w);
  }

  private resetText(w: UsageWindow): string {
    if (w.resetAt === null) return "---";
    const mins = Math.max(0, Math.round((w.resetAt * 1000 - Date.now()) / 60_000));
    return formatReset(mins);
  }

  private renderResets(): void {
    const s = this.snapshot;
    if (!s) return;
    if (s.fiveHour) this.panels.session.reset.textContent = this.resetText(s.fiveHour);
    if (s.sevenDay) this.panels.weekly.reset.textContent = this.resetText(s.sevenDay);
  }
}
