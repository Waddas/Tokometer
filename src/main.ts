import "./styles.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { UsageRenderer } from "./usage";
import { Splash } from "./splash";
import { RateTracker } from "./rate";

const appWindow = getCurrentWindow();

const root = document.getElementById("root")!;
const mascotCanvas = document.getElementById("mascot") as HTMLCanvasElement;
const btnPin = document.getElementById("btn-pin")!;
const btnRefresh = document.getElementById("btn-refresh")!;
const btnHide = document.getElementById("btn-hide")!;

const usage = new UsageRenderer(document.body);
const splash = new Splash(mascotCanvas);
const rate = new RateTracker();
splash.start();

/* ---- design-space scaling (282x168 -> window size) ---- */
function updateScale() {
  document.documentElement.style.setProperty("--scale", String(window.innerWidth / 282));
}
window.addEventListener("resize", updateScale);
updateScale();

/* ---- drag to move ---- */
root.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  void appWindow.startDragging();
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
});

void api.getState().then((st) => {
  pinned = st.pin;
  renderPin();
  if (st.lastUsage) applySnapshot(st.lastUsage);
});
