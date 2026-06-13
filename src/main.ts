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
const btnHide = document.getElementById("btn-hide")!;

const usage = new UsageRenderer(document.body);
const splash = new Splash(mascotCanvas);
const rate = new RateTracker();
const history = new UsageHistory();

const graph = new UsageGraph(document.getElementById("graph") as HTMLCanvasElement, history);

/* ---- layouts ----
 * Each layout has its own design-space width (geometry in styles.css); the
 * window is the design space scaled by the chosen Size (factors in state.rs).
 * `--scale` is derived from the resized width, so any size fills correctly. */
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
  document.documentElement.style.setProperty(
    "--scale",
    String(window.innerWidth / DESIGN_WIDTH[layout]),
  );
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
for (const btn of [btnPin, btnRefresh, btnHide]) {
  btn.addEventListener("mousedown", (e) => e.stopPropagation());
}
btnPin.addEventListener("click", () => void api.setPin(!pinned));
btnRefresh.addEventListener("click", () => void api.refreshNow());
btnHide.addEventListener("click", () => void api.toggleVisibility());

/* ---- data wiring ---- */
let mockActive = false;
let lastReal: api.UsageSnapshot | null = null;

function applySnapshot(s: api.UsageSnapshot) {
  usage.update(s);
  if (!mockActive) history.sample(s);
  graph.update(s);
  if (s.status === "ok" && s.fiveHour) {
    rate.sample(s.fiveHour.utilization);
    splash.setGroup(rate.group());
  }
}

void api.onUsage((s) => {
  lastReal = s;
  if (!mockActive) applySnapshot(s);
});

/* ---- dev: D toggles dev mode, shown as a badge in the top strip. While on,
 * M toggles mocked data and A cycles the mascot animation; leaving dev mode
 * resets both. ---- */
if (import.meta.env.DEV) {
  let devMode = false;
  let pinnedAnim = -1; // -1 = automatic rate-grouped rotation

  const badge = document.createElement("div");
  badge.id = "dev-badge";
  badge.hidden = true;
  document.body.appendChild(badge);

  function renderBadge() {
    badge.hidden = !devMode;
    const anim = pinnedAnim === -1 ? "auto" : splash.animationNames()[pinnedAnim];
    badge.textContent = `dev · ${mockActive ? "mock" : "live"} · ${anim}`;
  }

  const setMock = (on: boolean) =>
    import("./mock").then(({ MockHistory }) => {
      if (mockActive === on) return;
      mockActive = on;
      if (on) {
        const mock = new MockHistory();
        graph.setHistory(mock);
        applySnapshot(mock.snapshot);
      } else {
        graph.setHistory(history);
        if (lastReal) applySnapshot(lastReal);
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
          void setMock(false);
          setAnim(-1);
        }
        renderBadge();
        break;
      case "m":
        if (devMode) void setMock(!mockActive);
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
