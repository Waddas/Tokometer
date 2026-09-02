// Settings window: every preference in one place, replacing the old tray
// submenus. Renders from get_state, applies through the same single-mutation
// commands the widget uses, and re-renders on state://change so edits made
// from the widget (pin button, corner resize) stay in sync.
import "./settings.css";
import { getVersion } from "@tauri-apps/api/app";
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

/* ---- beta features: one checkbox per flag, sent as the whole struct ---- */
let beta: api.BetaFeatures = { learnedForecast: false };
const BETA_BOXES: [keyof api.BetaFeatures, HTMLInputElement][] = [
  ["learnedForecast", document.getElementById("beta-learned-forecast") as HTMLInputElement],
];
for (const [flag, box] of BETA_BOXES) {
  box.addEventListener("change", () => {
    beta = { ...beta, [flag]: box.checked };
    void api.setBeta(beta);
  });
}

/* ---- limits: one toggle per window the last poll reported; unchecking one
 * hides its tile. Sent as the whole hidden-id array. New limits arrive checked,
 * so a limit the API starts reporting shows up on its own. ---- */
const limitsBox = document.getElementById("limits")!;
const limitsHint = document.getElementById("limits-hint")!;
let hiddenLimits: string[] = [];
let limitWindows: api.LimitWindow[] = [];

function renderLimits() {
  limitsBox.replaceChildren();
  limitsHint.hidden = limitWindows.length > 0;
  for (const w of limitWindows) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = !hiddenLimits.includes(w.id);
    label.classList.toggle("on", input.checked);
    label.appendChild(input);
    label.appendChild(document.createTextNode(w.label));
    input.addEventListener("change", () => {
      hiddenLimits = input.checked
        ? hiddenLimits.filter((id) => id !== w.id)
        : [...hiddenLimits, w.id];
      void api.setHiddenLimits(hiddenLimits);
      label.classList.toggle("on", input.checked);
    });
    limitsBox.appendChild(label);
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
  hiddenLimits = [...prefs.hiddenLimits];
  renderLimits();
  beta = { ...prefs.beta };
  for (const [flag, box] of BETA_BOXES) box.checked = prefs.beta[flag];
}

// The window is created hidden (commands.rs) because the webview flashes
// white before first paint; reveal it once the first render is in — or
// regardless if get_state fails, so the window can never stay invisible.
// No waiting on requestAnimationFrame here: frames don't tick while the
// webview is hidden, so its callback would only run after something else
// showed the window.
void api
  .getState()
  .then((st) => {
    limitWindows = st.lastUsage?.windows ?? [];
    render(st);
  })
  .finally(() => {
    const win = getCurrentWindow();
    void win.show().then(() => win.setFocus());
  });
void api.onStateChange(render);
// A poll can detect a new limit while this window is open.
void api.onUsage((s) => {
  if (s.status !== "ok") return;
  limitWindows = s.windows;
  renderLimits();
});
void api.getAutostart().then((on) => (autostartBox.checked = on));
void getVersion().then((v) => {
  document.getElementById("version")!.textContent = `Tokometer ${v}`;
});

/* ---- updates: the button checks, then offers the release a check found ---- */
const updateBtn = document.getElementById("update-btn") as HTMLButtonElement;
const updateDismiss = document.getElementById("update-dismiss")!;
const updateHint = document.getElementById("update-hint")!;
let updateAvailable = false;

updateBtn.addEventListener("click", () => {
  void (updateAvailable ? api.installUpdate() : api.checkForUpdates());
});
updateDismiss.addEventListener("click", () => void api.dismissUpdate());

function updateLabel(u: api.UpdatePhase): string {
  switch (u.phase) {
    case "checking":
      return "Checking…";
    case "available":
      return `Update to ${u.version}`;
    case "installing":
      return `Installing ${u.version}…`;
    default:
      return "Check for updates";
  }
}

function updateNote(u: api.UpdatePhase): string {
  switch (u.phase) {
    case "available":
      return "Downloads, installs and relaunches.";
    case "up-to-date":
      return "You're on the latest version.";
    case "failed":
      return u.reason;
    default:
      return "";
  }
}

void api.onUpdatePhase((u) => {
  updateAvailable = u.phase === "available";
  updateBtn.disabled = u.phase === "checking" || u.phase === "installing";
  updateBtn.classList.toggle("selected", updateAvailable);
  updateBtn.textContent = updateLabel(u);
  updateDismiss.hidden = !(u.phase === "available" && !u.dismissed);
  const note = updateNote(u);
  updateHint.textContent = note;
  updateHint.hidden = note === "";
});
