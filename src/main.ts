import "./styles.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { UsageRenderer } from "./usage";
import { Splash } from "./splash";
import { RateTracker } from "./rate";
import { UsageHistory } from "./history";
import { UsageGraph } from "./graph";

const appWindow = getCurrentWindow();

const root = document.getElementById("root")!;
const mascotCanvas = document.getElementById("mascot") as HTMLCanvasElement;
const btnPin = document.getElementById("btn-pin")!;
const btnRefresh = document.getElementById("btn-refresh")!;
const btnSettings = document.getElementById("btn-settings")!;
const btnHide = document.getElementById("btn-hide")!;
const statusEl = document.getElementById("status")!;

const usage = new UsageRenderer(document.body);
const splash = new Splash(mascotCanvas);
const rate = new RateTracker();
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

function updateScale() {
  const scale = window.innerWidth / DESIGN_WIDTH[layout];
  document.documentElement.style.setProperty("--chrome", String(Math.min(1, scale)));
}
window.addEventListener("resize", updateScale);

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
  const startWidth = window.innerWidth;
  let width = startWidth;
  let raf = 0;
  const onMove = (ev: PointerEvent) => {
    width = startWidth + (ev.screenX - startX);
    // One resize per frame; the invoke is async and moves arrive faster.
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        void api.resizeWidget(width, false);
      });
    }
  };
  const onUp = () => {
    grip.removeEventListener("pointermove", onMove);
    grip.removeEventListener("pointerup", onUp);
    grip.removeEventListener("pointercancel", onUp);
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    void api.resizeWidget(width, true);
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

/* ---- status line: friendly guidance when polling fails ---- */
// Kept terse: the widget can be very narrow, and the raw error sits in the
// element's tooltip for anyone who wants the details.
function friendlyError(err: string): string {
  if (err.includes("no Claude credentials")) return "Sign in to Claude Code to start tracking";
  if (err.startsWith("token expired")) return "Token expired — open Claude Code";
  return "Can't reach usage API — retrying";
}

function renderStatus(s: api.UsageSnapshot) {
  const failing = s.status !== "ok";
  statusEl.hidden = !failing;
  // The content grid reserves a band for the bar while it's up (styles.css).
  root.classList.toggle("has-status", failing);
  if (!failing) return;
  statusEl.textContent = friendlyError(s.error ?? "");
  statusEl.title = s.error ?? "";
}

/* ---- data wiring ---- */
let mockActive = false;
let lastReal: api.UsageSnapshot | null = null;

function applySnapshot(s: api.UsageSnapshot) {
  usage.update(s);
  renderStatus(s);
  if (!mockActive) history.sample(s);
  graph.update(s);
  if (s.status === "ok" && s.fiveHour) {
    rate.sample(s.fiveHour.utilization);
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
  try {
    history.load(await api.getHistory());
    graph.redraw();
  } catch {
    // Backend unavailable; live sampling still fills the graph.
  }
}
void initHistory();

void api.onUsage((s) => {
  lastReal = s;
  if (!mockActive) applySnapshot(s);
});

/* ---- dev: D toggles dev mode, shown as a badge in the top strip. While on,
 * M cycles the data source (live → mock → error) and A cycles the mascot
 * animation; leaving dev mode resets both. ---- */
if (import.meta.env.DEV) {
  let devMode = false;
  let pinnedAnim = -1; // -1 = automatic rate-grouped rotation
  let barHidden = false; // tray "Hide dev bar" — keeps dev mode on for captures
  const SOURCES = ["live", "mock", "error"] as const;
  let devSource: (typeof SOURCES)[number] = "live";

  const badge = document.createElement("div");
  badge.id = "dev-badge";
  badge.hidden = true;
  document.body.appendChild(badge);

  function renderBadge() {
    badge.hidden = !devMode || barHidden;
    const anim = pinnedAnim === -1 ? "auto" : splash.animationNames()[pinnedAnim];
    badge.textContent = `dev · ${devSource} · ${anim}`;
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
    fiveHour: null,
    sevenDay: null,
    fiveHourStatus: null,
    error: "mocked failure (dev): usage API unreachable",
  });

  const setSource = (src: (typeof SOURCES)[number]) =>
    import("./mock").then(({ MockHistory }) => {
      if (devSource === src) return;
      devSource = src;
      mockActive = src !== "live";
      if (src === "mock") {
        const mock = new MockHistory();
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

  window.addEventListener("keydown", (e) => {
    if (e.repeat) return;
    switch (e.key.toLowerCase()) {
      case "d":
        devMode = !devMode;
        if (!devMode) {
          void setSource("live");
          setAnim(-1);
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
    }
  });
}
void api.onStateChange((s) => {
  pinned = s.pin;
  renderPin();
  applyLayout(s.layout);
  splash.setMascot(s.mascot);
  markMascot(s.mascot);
  graph.setWorkDays(s.workDays);
});

void api.getState().then((st) => {
  pinned = st.pin;
  renderPin();
  applyLayout(st.layout);
  splash.setMascot(st.mascot);
  markMascot(st.mascot);
  graph.setWorkDays(st.workDays);
  if (st.lastUsage) {
    lastReal = st.lastUsage;
    if (!mockActive) applySnapshot(st.lastUsage);
  }
});
