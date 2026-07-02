// Settings window: every preference in one place, replacing the old tray
// submenus. Renders from get_state, applies through the same single-mutation
// commands the widget uses, and re-renders on state://change so edits made
// from the widget (pin button, corner resize) stay in sync.
import "./settings.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";

const LAYOUTS: [api.Layout, string][] = [
  ["mascot-left", "Display left"],
  ["mascot-right", "Display right"],
  ["mascot-top", "Display top"],
  ["mascot-bottom", "Display bottom"],
  ["tiles-row", "Tiles only (wide)"],
  ["tiles-column", "Tiles only (tall)"],
];
const SIZES: [api.Size, string][] = [
  ["small", "Small"],
  ["medium", "Medium"],
  ["large", "Large"],
];
const MASCOTS: [api.Mascot, string][] = [
  ["clawd", "Clawd"],
  ["axolotl", "Axolotl"],
  ["cat", "Cat"],
];
const TRAY_STYLES: [api.TrayStyle, string][] = [
  ["ring", "Ring"],
  ["text", "Text"],
];
// Shown Monday-first; each maps to its Sun..Sat index to match Date.getDay().
const WORK_DAYS: [number, string][] = [
  [1, "Mon"],
  [2, "Tue"],
  [3, "Wed"],
  [4, "Thu"],
  [5, "Fri"],
  [6, "Sat"],
  [0, "Sun"],
];

/** A row of radio-style buttons; returns a function that marks the selection. */
function optionGroup<T extends string>(
  containerId: string,
  options: [T, string][],
  pick: (value: T) => void,
): (selected: T | null) => void {
  const container = document.getElementById(containerId)!;
  const buttons = new Map<T, HTMLButtonElement>();
  for (const [value, label] of options) {
    const btn = document.createElement("button");
    btn.textContent = label;
    btn.addEventListener("click", () => pick(value));
    buttons.set(value, btn);
    container.appendChild(btn);
  }
  return (selected) => {
    for (const [value, btn] of buttons) btn.classList.toggle("selected", value === selected);
  };
}

const markLayout = optionGroup("opt-layout", LAYOUTS, (l) => void api.setLayout(l));
const markSize = optionGroup("opt-size", SIZES, (s) => void api.setSize(s));
const markMascot = optionGroup("opt-mascot", MASCOTS, (m) => void api.setMascot(m));
const markTray = optionGroup("opt-tray", TRAY_STYLES, (t) => void api.setTrayStyle(t));

const sizeHint = document.getElementById("size-hint")!;
const pinBox = document.getElementById("pin") as HTMLInputElement;
const autostartBox = document.getElementById("autostart") as HTMLInputElement;
const probeBox = document.getElementById("probe") as HTMLInputElement;

pinBox.addEventListener("change", () => void api.setPin(pinBox.checked));
autostartBox.addEventListener(
  "change",
  () => void api.setAutostart(autostartBox.checked).then((on) => (autostartBox.checked = on)),
);
probeBox.addEventListener("change", () => void api.setProbeFallback(probeBox.checked));

/* ---- work days: independent toggles, sent as the whole Sun..Sat array ---- */
let workDays = [true, true, true, true, true, true, true];
const dayBoxes = new Map<number, { label: HTMLLabelElement; input: HTMLInputElement }>();
{
  const container = document.getElementById("days")!;
  for (const [day, name] of WORK_DAYS) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    label.appendChild(input);
    label.appendChild(document.createTextNode(name));
    input.addEventListener("change", () => {
      workDays = workDays.map((on, i) => (i === day ? input.checked : on));
      void api.setWorkDays(workDays);
      label.classList.toggle("on", input.checked);
    });
    dayBoxes.set(day, { label, input });
    container.appendChild(label);
  }
}

function render(prefs: api.Preferences) {
  markLayout(prefs.layout);
  // A free-resized widget matches no preset; say what it is instead.
  markSize(prefs.customScale === null ? prefs.size : null);
  sizeHint.textContent =
    prefs.customScale === null
      ? "Or drag the widget's bottom-right grip to any size."
      : `Custom size (${prefs.customScale.toFixed(2)}×) — pick a preset to reset.`;
  markMascot(prefs.mascot);
  markTray(prefs.trayStyle);
  pinBox.checked = prefs.pin;
  probeBox.checked = prefs.probeFallback;
  workDays = [...prefs.workDays];
  for (const [day, { label, input }] of dayBoxes) {
    input.checked = prefs.workDays[day];
    label.classList.toggle("on", prefs.workDays[day]);
  }
}

// The window is created hidden (commands.rs) because the webview flashes
// white before first paint; reveal it once the first render is on screen —
// or regardless if get_state fails, so the window can never stay invisible.
void api
  .getState()
  .then(render)
  .finally(() => {
    requestAnimationFrame(() => {
      const win = getCurrentWindow();
      void win.show().then(() => win.setFocus());
    });
  });
void api.onStateChange(render);
void api.getAutostart().then((on) => (autostartBox.checked = on));
