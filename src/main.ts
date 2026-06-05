import "./styles.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { UsageRenderer } from "./usage";
import { Splash } from "./splash";
import { RateTracker } from "./rate";

const appWindow = getCurrentWindow();

const root = document.getElementById("root")!;
const usageEl = document.getElementById("usage-view")!;
const splashCanvas = document.getElementById("splash-view") as HTMLCanvasElement;
const btnCompact = document.getElementById("btn-compact")!;
const btnPin = document.getElementById("btn-pin")!;
const btnRefresh = document.getElementById("btn-refresh")!;
const btnHide = document.getElementById("btn-hide")!;

const usage = new UsageRenderer(usageEl);
const splash = new Splash(splashCanvas);
const rate = new RateTracker();

/* ---- design-space scaling (mode's design width -> window width) ---- */
let compact = false;
function updateScale() {
  const designWidth = compact ? 282 : 480;
  document.documentElement.style.setProperty("--scale", String(window.innerWidth / designWidth));
}
window.addEventListener("resize", updateScale);
updateScale();

/* ---- view toggle (mirrors tapping the Clawdmeter screen) ---- */
let view: "usage" | "splash" = "usage";
function setView(v: typeof view) {
  view = v;
  usageEl.hidden = v !== "usage";
  splashCanvas.hidden = v !== "splash";
  if (v === "splash") splash.start();
  else splash.stop();
}

/* ---- compact mode (mascot + slim usage rows, half-height window) ---- */
function applyCompact(c: boolean) {
  compact = c;
  document.body.classList.toggle("compact", c);
  updateScale();
  usage.setCompact(c);
  btnCompact.textContent = c ? "⊞" : "⊟";
  btnCompact.title = c ? "Expand" : "Compact view";
  if (c) {
    // Both views render side by side; the mascot is always animating.
    usageEl.hidden = false;
    splashCanvas.hidden = false;
    splash.start();
  } else {
    setView(view);
  }
}

/* ---- drag vs click gesture ----
 * mousedown records the origin; moving past the threshold hands the gesture
 * to the OS via startDragging(), a clean release toggles splash <-> usage. */
const DRAG_THRESHOLD = 4;
let pressed = false;
root.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  pressed = true;
  const sx = e.clientX;
  const sy = e.clientY;
  const onMove = (ev: MouseEvent) => {
    if (!pressed) return;
    if (Math.abs(ev.clientX - sx) > DRAG_THRESHOLD || Math.abs(ev.clientY - sy) > DRAG_THRESHOLD) {
      pressed = false;
      cleanup();
      void appWindow.startDragging();
    }
  };
  const onUp = () => {
    if (pressed && !compact) setView(view === "usage" ? "splash" : "usage");
    pressed = false;
    cleanup();
  };
  const cleanup = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
});

/* ---- hover controls ---- */
let pinned = false;
function renderPin() {
  btnPin.classList.toggle("pinned", pinned);
  btnPin.title = pinned ? "Unpin" : "Pin on top";
}
for (const btn of [btnCompact, btnPin, btnRefresh, btnHide]) {
  btn.addEventListener("mousedown", (e) => e.stopPropagation());
}
btnCompact.addEventListener("click", () => void api.setCompact(!compact));
btnPin.addEventListener("click", () => void api.setPin(!pinned));
btnRefresh.addEventListener("click", () => void api.refreshNow());
btnHide.addEventListener("click", () => void api.toggleVisibility());

/* ---- data wiring ---- */
function applySnapshot(s: api.UsageSnapshot) {
  usage.update(s);
  if (s.status === "ok" && s.fiveHour) {
    rate.sample(s.fiveHour.utilization);
    splash.setGroup(rate.group());
  }
}

void api.onUsage(applySnapshot);
void api.onStateChange((s) => {
  pinned = s.pin;
  renderPin();
  if (s.compact !== compact) applyCompact(s.compact);
});

void api.getState().then((st) => {
  pinned = st.pin;
  renderPin();
  if (st.compact) applyCompact(st.compact);
  if (st.lastUsage) applySnapshot(st.lastUsage);
});
