import "./styles.css";
import { usageStatus } from "./status";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { UsageRenderer } from "./usage";
import { Splash } from "./splash";
import { RateTracker } from "./rate";
import { UsageHistory } from "./history";
import { UsageGraph } from "./graph";
import { applyTheme, restoreTheme } from "./theme";

restoreTheme();

const appWindow = getCurrentWindow();

const root = document.getElementById("root")!;
const content = document.getElementById("content")!;
const mascotCanvas = document.getElementById("mascot") as HTMLCanvasElement;
const btnPin = document.getElementById("btn-pin")!;
const btnRefresh = document.getElementById("btn-refresh")!;
const btnSettings = document.getElementById("btn-settings")!;
const btnHide = document.getElementById("btn-hide")!;
const statusEl = document.getElementById("status")!;

const splash = new Splash(mascotCanvas);
let rate = new RateTracker();
const history = new UsageHistory();

const graph = new UsageGraph(document.getElementById("graph") as HTMLCanvasElement, history);

/* ---- layouts ----
 * Each layout has its own design-space width (geometry in styles.css); the
 * window is the design space scaled by the chosen Size (factors in state.rs).
 * `--chrome` is derived from the resized width: margins, gaps and radii track
 * small widgets down but stop growing past design size, so large widgets put
 * the room into content instead of bezels. */
const DESIGN_WIDTH: Record<api.Layout, number> = {
  "mascot-left": 282,
  "mascot-right": 282,
  "mascot-top": 238,
  "mascot-bottom": 238,
  "tiles-row": 238,
  "tiles-column": 128,
};

let layout: api.Layout = "mascot-left";
let geometry: api.ChromeGeometry | null = null;

function applyGeometry(g: api.ChromeGeometry | null | undefined) {
  if (!g) return;
  geometry = g;
  const place = (element: HTMLElement, rect: api.ChromeRect) => {
    Object.assign(element.style, { left: `${rect.x}px`, top: `${rect.y}px`,
      width: `${rect.width}px`, height: `${rect.height}px` });
  };
  place(root, g.widget);
  for (const [id, bar] of [["controls", g.controls], ["provider-controls", g.providers]] as const) {
    const element = document.getElementById(id)!;
    place(element, bar.rect);
    element.style.setProperty("--columns", String(bar.columns));
    element.style.setProperty("--rows", String(bar.rows));
    element.classList.toggle("vertical", bar.vertical);
    element.hidden = bar.buttons.length === 0;
    [...element.querySelectorAll<HTMLButtonElement>(":scope > button")].forEach((button, i) => {
      if (bar.buttons[i]) place(button, bar.buttons[i]);
    });
  }
  updateScale();
  graph.redraw();
}
let resizeRequest = 0;
function resizeWidget(width: number, commit: boolean) {
  const request = ++resizeRequest;
  void api.resizeWidget(width, commit).then((g) => {
    if (request === resizeRequest) applyGeometry(g);
  });
}

function updateScale() {
  const scale = (geometry?.widget.width ?? root.clientWidth) / DESIGN_WIDTH[layout];
  document.documentElement.style.setProperty("--chrome", String(Math.min(1, scale)));
}
window.addEventListener("resize", updateScale);

/** How many tile tracks the content grid repeats: the tiles share the
 *  layout's fixed band, so the window size never depends on the count. */
function setTiles(count: number) {
  content.style.setProperty("--tiles", String(count));
}

const usage = new UsageRenderer(content, setTiles);

function applyLayout(l: api.Layout) {
  layout = l;
  document.body.className = `layout-${l}`;
  updateScale();
  updateSplashRunning();
}

/* ---- drag to move (widget body or the grab handle) ---- */
root.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  void appWindow.startDragging();
});
document.getElementById("drag-handle")!.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  void appWindow.startDragging();
});

/* ---- drag the corner grip to resize. The drag is ours, not the OS's: each
 * pointer move asks the backend for a width-driven, aspect-locked size, so
 * the widget can never be stretched out of shape mid-drag; releasing commits
 * the resulting scale. Pointer capture keeps the moves flowing even though
 * the grip slides out from under a fast cursor between resizes. ---- */
const grip = document.getElementById("resize-handle")!;
grip.addEventListener("mousedown", (e) => e.stopPropagation()); // no move-drag underneath
grip.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  grip.setPointerCapture(e.pointerId);
  const startX = e.screenX; // screen coords: stable while the window resizes
  const startWidth = root.clientWidth;
  let width = startWidth;
  let raf = 0;
  const onMove = (ev: PointerEvent) => {
    width = startWidth + (ev.screenX - startX);
    // One resize per frame; the invoke is async and moves arrive faster.
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        resizeWidget(width, false);
      });
    }
  };
  const onUp = () => {
    grip.removeEventListener("pointermove", onMove);
    grip.removeEventListener("pointerup", onUp);
    grip.removeEventListener("pointercancel", onUp);
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    resizeWidget(width, true);
  };
  grip.addEventListener("pointermove", onMove);
  grip.addEventListener("pointerup", onUp);
  grip.addEventListener("pointercancel", onUp);
});

/* ---- mascot chip flips between mascot and graph on click ---- */
const mascotChip = document.getElementById("mascot-chip")!;
const CHIP_KEY = "mascot-graph";

// The mascot only animates while a layout shows it and the graph isn't open.
function updateSplashRunning() {
  const visible =
    layout !== "tiles-row" &&
    layout !== "tiles-column" &&
    !mascotChip.classList.contains("show-graph");
  if (visible) splash.start();
  else splash.stop();
}

if (localStorage.getItem(CHIP_KEY) === "1") mascotChip.classList.add("show-graph");
applyLayout(layout);

mascotChip.addEventListener("mousedown", (e) => e.stopPropagation());
mascotChip.addEventListener("click", () => {
  mascotMenu.hidden = true; // chip clicks don't bubble to the menu-closing handler
  const showing = mascotChip.classList.toggle("show-graph");
  localStorage.setItem(CHIP_KEY, showing ? "1" : "0");
  updateSplashRunning();
});

/* ---- mascot picker (right-click the mascot) ---- */
const mascotMenu = document.getElementById("mascot-menu")!;
const mascotButtons = new Map<api.Mascot, HTMLButtonElement>();
for (const m of ["clawd", "axolotl", "cat"] as const) {
  const btn = document.createElement("button");
  btn.textContent = m[0].toUpperCase() + m.slice(1);
  btn.addEventListener("mousedown", (e) => e.stopPropagation());
  btn.addEventListener("click", () => {
    mascotMenu.hidden = true;
    void api.setMascot(m);
  });
  mascotButtons.set(m, btn);
  mascotMenu.appendChild(btn);
}

function markMascot(current: api.Mascot) {
  for (const [id, btn] of mascotButtons) btn.classList.toggle("selected", id === current);
}

window.addEventListener("contextmenu", (e) => {
  e.preventDefault(); // right-click does nothing anywhere else
  if (!mascotChip.contains(e.target as Node)) {
    mascotMenu.hidden = true;
    return;
  }
  mascotMenu.hidden = false;
  const { offsetWidth: w, offsetHeight: h } = mascotMenu;
  mascotMenu.style.left = `${Math.min(e.clientX, window.innerWidth - w - 4)}px`;
  mascotMenu.style.top = `${Math.min(e.clientY, window.innerHeight - h - 4)}px`;
});
window.addEventListener("mousedown", (e) => {
  if (!mascotMenu.contains(e.target as Node)) mascotMenu.hidden = true;
});

/* ---- hover controls ---- */
let pinned = false;
function renderPin() {
  btnPin.classList.toggle("pinned", pinned);
  btnPin.title = pinned ? "Unpin" : "Pin on top";
}
for (const btn of [btnPin, btnRefresh, btnSettings, btnHide]) {
  btn.addEventListener("mousedown", (e) => e.stopPropagation());
}
btnPin.addEventListener("click", () => void api.setPin(!pinned));
btnRefresh.addEventListener("click", () => void api.refreshNow());
btnSettings.addEventListener("click", () => void api.openSettings());
btnHide.addEventListener("click", () => void api.toggleVisibility());

const providerButtons = new Map<api.Provider, HTMLButtonElement>();
for (const id of ["claude", "codex"] as const) {
  const button = document.getElementById(`provider-${id}`) as HTMLButtonElement;
  button.addEventListener("mousedown", (e) => e.stopPropagation());
  button.addEventListener("click", () => void api.setProvider(id));
  providerButtons.set(id, button);
}

/* ---- update available: a dot on the settings gear; settings offers the install ---- */
const updateDot = document.getElementById("update-dot")!;
void api.onUpdatePhase((u) => {
  const offered = u.phase === "available" && !u.dismissed;
  updateDot.hidden = !offered;
  btnSettings.title = offered ? `Settings — update ${u.version} available` : "Settings";
});

/* ---- status line: friendly guidance when polling fails ---- */
let statusSnapshot: api.UsageSnapshot | null = null;
// Only the displayed age/countdown ticks; this never triggers a network request.
setInterval(() => { if (statusSnapshot) renderStatus(statusSnapshot); }, 30_000);

function renderStatus(s: api.UsageSnapshot) {
  statusSnapshot = s;
  const failing = s.status !== "ok";
  statusEl.hidden = !failing;
  // The content grid reserves a band for the bar while it's up (styles.css).
  root.classList.toggle("has-status", failing);
  if (!failing) return;
  statusEl.textContent = usageStatus(s);
  statusEl.title = `${usageStatus(s)}\n${s.error ?? ""}`;
}

/* ---- data wiring ---- */
let mockActive = false;
let lastReal: api.UsageSnapshot | null = null;
/** Last successful poll — what stays on screen, greyed, while polling fails. */
let lastOk: api.UsageSnapshot | null = null;
let provider: api.Provider = "claude";
let scope = "claude";

function selectScope(next: string) {
  if (next === scope) return;
  scope = next;
  lastOk = null;
  rate = new RateTracker();
  splash.setGroup(0);
  history.selectScope(next);
  void loadHistory(next);
}

function selectProvider(next: api.Provider) {
  const changed = provider !== next;
  provider = next;
  for (const [id, button] of providerButtons) button.setAttribute("aria-pressed", String(id === next));
  const name = next === "codex" ? "Codex" : "Claude";
  root.setAttribute("aria-label", `${name} usage`);
  btnRefresh.title = `Refresh ${name} usage`;
  if (!changed) return;
  lastReal = null;
  selectScope(next === "claude" ? "claude" : "codex:unknown");
  if (!mockActive) {
    applySnapshot({
      provider: next, scope, status: "error", source: null,
      fetchedAt: 0, windows: [], error: `Loading ${name} usage…`,
    });
  }
}

function applySnapshot(s: api.UsageSnapshot) {
  if (!mockActive) selectScope(s.scope ?? "claude");
  const stale = s.status !== "ok";
  if (!stale && !mockActive) lastOk = s;
  // Keep the same account’s last values visible on failures. The backend also
  // preserves stale windows so they remain available after restarting.
  const shown = stale && lastOk && lastOk.scope === s.scope ? lastOk : s;
  usage.update(shown, stale);
  root.classList.toggle("stale", stale);
  renderStatus(s);
  if (!mockActive) history.sample(s);
  graph.update(shown);
  const session = s.windows.find((w) => w.id === api.SESSION_ID);
  if (s.status === "ok" && session) {
    rate.sample(session.utilization);
    splash.setGroup(rate.group());
  }
}

/* ---- history: the backend owns the log; mirror it, migrating any samples
 * the pre-backend build left in localStorage ---- */
const LEGACY_HISTORY_KEY = "usage-history";
async function initHistory() {
  const legacy = localStorage.getItem(LEGACY_HISTORY_KEY);
  if (legacy) {
    try {
      await api.importHistory(JSON.parse(legacy) as api.HistorySample[]);
    } catch {
      // Unparseable or rejected — nothing worth keeping.
    }
    localStorage.removeItem(LEGACY_HISTORY_KEY);
  }
  await loadHistory(scope);
}
async function loadHistory(requestedScope: string) {
  try {
    const samples = await api.getHistory(requestedScope);
    history.loadForScope(requestedScope, samples);
    if (scope === requestedScope) graph.redraw();
  } catch { /* Live samples remain available if the history request fails. */ }
}
void initHistory();

void api.onUsage((s) => {
  if ((s.provider ?? "claude") !== provider) return;
  lastReal = s;
  if (!mockActive) applySnapshot(s);
});

/* ---- dev: D toggles dev mode, shown as a badge in the top strip. While on,
 * M cycles the data source (live → mock → mock-stale → error), A cycles the
 * mascot animation and U offers a mock update; leaving dev mode resets all. ---- */
if (import.meta.env.DEV) {
  let devMode = false;
  let pinnedAnim = -1; // -1 = automatic rate-grouped rotation
  let barHidden = false; // tray "Hide dev bar" — keeps dev mode on for captures
  let updateMocked = false;
  // "mock-stale" is the same mock served by the fallback probe: its scoped
  // window is a carried-over value, so that tile renders dimmed.
  const SOURCES = ["live", "mock", "mock-stale", "error"] as const;
  let devSource: (typeof SOURCES)[number] = "live";

  const badge = document.createElement("div");
  badge.id = "dev-badge";
  badge.hidden = true;
  document.body.appendChild(badge);

  function renderBadge() {
    badge.hidden = !devMode || barHidden;
    const anim = pinnedAnim === -1 ? "auto" : splash.animationNames()[pinnedAnim];
    badge.textContent = `dev · ${devSource} · ${anim}${updateMocked ? " · update" : ""}`;
  }

  void api.onDevBarHidden((hidden) => {
    barHidden = hidden;
    renderBadge();
  });

  // A snapshot shaped like a failed poll, for iterating on the error UX.
  const errorSnapshot = (): api.UsageSnapshot => ({
    status: "error",
    source: null,
    fetchedAt: Date.now(),
    windows: [],
    error: "mocked failure (dev): usage API unreachable",
  });

  const setSource = (src: (typeof SOURCES)[number]) =>
    import("./mock").then(({ MockHistory }) => {
      if (devSource === src) return;
      devSource = src;
      mockActive = src !== "live";
      if (src === "mock" || src === "mock-stale") {
        const mock = new MockHistory(Date.now(), src === "mock" ? "fresh" : "stale-scoped");
        graph.setHistory(mock);
        applySnapshot(mock.snapshot);
        void api.setTrayOverride(mock.snapshot);
      } else if (src === "error") {
        // The real history stays under the graph, as it would on a live
        // failure; only the snapshot reports the outage.
        graph.setHistory(history);
        const snap = errorSnapshot();
        applySnapshot(snap);
        void api.setTrayOverride(snap);
      } else {
        graph.setHistory(history);
        if (lastReal) applySnapshot(lastReal);
        void api.setTrayOverride(null);
      }
      renderBadge();
    });

  function setAnim(idx: number) {
    pinnedAnim = idx;
    splash.setAnimation(idx === -1 ? null : splash.animationNames()[idx]);
    renderBadge();
  }

  function setUpdateMock(on: boolean) {
    updateMocked = on;
    void api.setUpdateOverride(on);
    renderBadge();
  }

  window.addEventListener("keydown", (e) => {
    if (e.repeat) return;
    switch (e.key.toLowerCase()) {
      case "d":
        devMode = !devMode;
        if (!devMode) {
          void setSource("live");
          setAnim(-1);
          setUpdateMock(false);
        }
        renderBadge();
        break;
      case "m":
        if (devMode)
          void setSource(SOURCES[(SOURCES.indexOf(devSource) + 1) % SOURCES.length]);
        break;
      case "a":
        if (devMode) {
          const count = splash.animationNames().length;
          setAnim(pinnedAnim + 1 >= count ? -1 : pinnedAnim + 1);
        }
        break;
      case "u":
        if (devMode) setUpdateMock(!updateMocked);
        break;
    }
  });
}
let receivedStateChange = false;
void api.onStateChange((s) => {
  receivedStateChange = true;
  selectProvider(s.provider ?? "claude");
  applyTheme(s.theme);
  pinned = s.pin;
  renderPin();
  applyLayout(s.layout);
  applyGeometry(s.geometry);
  splash.setMascot(s.mascot);
  markMascot(s.mascot);
  graph.setWorkDays(s.workDays);
  graph.setHidden(s.hiddenLimits);
  usage.setHidden(s.hiddenLimits);
});

void api.getState().then((st) => {
  if (receivedStateChange) return;
  selectProvider(st.provider ?? "claude");
  applyTheme(st.theme);
  pinned = st.pin;
  renderPin();
  applyLayout(st.layout);
  applyGeometry(st.geometry);
  splash.setMascot(st.mascot);
  markMascot(st.mascot);
  graph.setWorkDays(st.workDays);
  graph.setHidden(st.hiddenLimits);
  usage.setHidden(st.hiddenLimits);
  if (st.lastUsage && (!lastReal || st.lastUsage.fetchedAt >= lastReal.fetchedAt)) {
    lastReal = st.lastUsage;
    if (!mockActive) applySnapshot(st.lastUsage);
  }
});
