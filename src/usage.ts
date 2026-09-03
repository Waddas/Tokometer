// Usage tile renderer — one labelled tile per limit window, threshold-coloured
// percentages and reset countdowns; thresholds and time format from
// Clawdmeter's firmware ui.cpp. Right-clicking a tile flips it to the info
// view: the limit's name over its countdown, no percentage.
import { DEFAULT_WINDOWS, type LimitWindow, type UsageSnapshot } from "./api";
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

/** Tiles showing the info view, as one comma-joined key (window ids never
 *  contain a comma — see `slug` in usage.rs). Mirrors graph.ts's mode key. */
const INFO_KEY = "tile-info-view";

/** Hover tooltip: which limit, when it resets, and whether the value is a
 *  carried-over one rather than this poll's. */
function infoText(label: string, reset: string, stale: boolean): string {
  const text = reset === "---" ? label : `${label} — resets in ${reset}`;
  return stale ? `${text} (last known)` : text;
}

interface PanelEls {
  root: HTMLElement;
  label: string;
  pct: HTMLElement;
  /** The countdown itself; its line also carries the folded-in label. */
  reset: HTMLElement;
}

/** One tile: a window, or a placeholder standing in for one. */
interface Tile {
  id: string;
  label: string;
  window: LimitWindow | null;
}

/** Shown until the first poll lands, so the widget is never blank. */
const PLACEHOLDERS: Tile[] = DEFAULT_WINDOWS.map((w) => ({ ...w, window: null }));

export class UsageRenderer {
  private panels = new Map<string, PanelEls>();
  private snapshot: UsageSnapshot | null = null;
  private stale = false;
  private hidden: string[] = [];
  /** The rendered tile ids, so unchanged polls don't rebuild the DOM. */
  private rendered = "";
  /** Tiles flipped to the info view, restored from the last session. */
  private infoTiles = new Set((localStorage.getItem(INFO_KEY) ?? "").split(",").filter(Boolean));

  constructor(
    private container: HTMLElement,
    /** Told the visible tile count whenever it changes (window sizing). */
    private onTiles: (count: number) => void,
  ) {
    this.render();
    // Keep reset countdowns fresh between polls.
    setInterval(() => this.renderResets(), 30_000);
  }

  /** Render a snapshot; `stale` drains the threshold colours to grey, for
   *  showing the last known values while polling fails. */
  update(s: UsageSnapshot, stale = false): void {
    this.snapshot = s;
    this.stale = stale;
    this.render();
  }

  /** Window ids the user hid in settings; those tiles are never rendered. */
  setHidden(ids: string[]): void {
    this.hidden = ids;
    this.render();
  }

  /** The tiles to show: the snapshot's visible windows, or the placeholders
   *  while no poll has landed. Mirrors `visible_tile_count` in state.rs. */
  private tiles(): Tile[] {
    const windows = this.snapshot?.windows ?? [];
    if (windows.length === 0) return PLACEHOLDERS;
    return windows
      .filter((w) => !this.hidden.includes(w.id))
      .map((w) => ({ id: w.id, label: w.label, window: w }));
  }

  private render(): void {
    const tiles = this.tiles();
    const ids = tiles.map((t) => `${t.id}|${t.label}`).join(";");
    if (ids !== this.rendered) {
      this.rendered = ids;
      for (const els of this.panels.values()) els.root.remove();
      this.panels.clear();
      for (const t of tiles) this.panels.set(t.id, this.createPanel(t));
      // Hiding every limit still leaves the widget one tile of room.
      this.onTiles(Math.max(1, tiles.length));
    }
    for (const t of tiles) this.renderPanel(this.panels.get(t.id)!, t.window);
  }

  /** A tile, appended to the content grid: it auto-flows after the mascot
   *  chip, so source order is the API's window order. */
  private createPanel(tile: Tile): PanelEls {
    const root = document.createElement("section");
    root.className = "panel";
    root.dataset.window = tile.id;
    const label = document.createElement("div");
    label.className = "label";
    label.textContent = tile.label;
    const pct = document.createElement("div");
    pct.className = "pct";
    // The label is rendered twice: on its own line for tall tiles, and as a
    // prefix to the countdown ("5h | 3h 8m") for tiles too short to read it
    // there. styles.css shows exactly one of the two.
    const resetLine = document.createElement("div");
    resetLine.className = "reset";
    const resetLabel = document.createElement("span");
    resetLabel.className = "reset-label";
    resetLabel.textContent = `${tile.label} | `;
    const reset = document.createElement("span");
    reset.className = "reset-value";
    resetLine.append(resetLabel, reset);
    root.append(label, pct, resetLine);
    root.classList.toggle("info", this.infoTiles.has(tile.id));
    // Right-click flips the tile's own content, like the graph's window switch.
    root.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation(); // the window-level handler only closes menus
      this.toggleInfo(tile.id);
    });
    this.container.appendChild(root);
    return { root, label: tile.label, pct, reset };
  }

  /** Flip one tile between the percentage and the name-over-countdown view;
   *  the info view is the only way to read both on a compact tile. */
  private toggleInfo(id: string): void {
    if (!this.infoTiles.delete(id)) this.infoTiles.add(id);
    localStorage.setItem(INFO_KEY, [...this.infoTiles].join(","));
    this.panels.get(id)?.root.classList.toggle("info", this.infoTiles.has(id));
  }

  private renderPanel(els: PanelEls, w: LimitWindow | null): void {
    const reset = w ? this.resetText(w) : "---";
    if (!w) {
      els.pct.textContent = "--%";
      els.pct.style.color = "var(--dim)";
    } else {
      const pct = Math.round(w.utilization);
      els.pct.textContent = `${pct}%`;
      // A carried-over window is dimmed even on a fresh poll: the rest of the
      // snapshot is live, this one value isn't.
      els.pct.style.color = this.stale || w.stale ? "var(--dim)" : pctColor(pct);
    }
    els.reset.textContent = reset;
    els.root.title = infoText(els.label, reset, w?.stale ?? false);
  }

  private resetText(w: LimitWindow): string {
    if (w.resetAt === null) return "---";
    const mins = Math.max(0, Math.round((w.resetAt * 1000 - Date.now()) / 60_000));
    return formatReset(mins);
  }

  private renderResets(): void {
    for (const w of this.snapshot?.windows ?? []) {
      const els = this.panels.get(w.id);
      if (!els) continue;
      const reset = this.resetText(w);
      els.reset.textContent = reset;
      els.root.title = infoText(els.label, reset, w.stale ?? false);
    }
  }
}
