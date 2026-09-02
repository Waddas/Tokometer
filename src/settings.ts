// Settings window: every preference in one place, replacing the old tray
// submenus. Renders from get_state, applies through the same single-mutation
// commands the widget uses, and re-renders on state://change so edits made
// from the widget (pin button, corner resize) stay in sync.
import "./settings.css";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "./api";
import { parseReleaseNotes } from "./release-notes";

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

/* ---- updates: the card's ring, title and button follow the phase; the
 * release notes and the dismiss link appear with an offered release, and the
 * ring draws the download while one installs ---- */
const updateCard = document.getElementById("update")!;
const updateTitle = document.getElementById("update-title")!;
const updateHint = document.getElementById("update-hint")!;
const updatePct = document.getElementById("update-pct")!;
const updateNotes = document.getElementById("update-notes")!;
const updateBtn = document.getElementById("update-btn") as HTMLButtonElement;
const updateDismiss = document.getElementById("update-dismiss")!;
let appVersion = "";
let updatePhase: api.UpdatePhase = { phase: "idle" };
/** Download fraction of the installing release; null until the first chunk. */
let updateProgress: number | null = null;

void getVersion().then((v) => {
  appVersion = v;
  renderUpdate();
});

updateBtn.addEventListener("click", () => {
  void (updatePhase.phase === "available" ? api.installUpdate() : api.checkForUpdates());
});
updateDismiss.addEventListener("click", () => void api.dismissUpdate());

function updateCopy(): [title: string, hint: string, button: string] {
  const u = updatePhase;
  switch (u.phase) {
    case "checking":
      return ["Checking for updates…", "", "Checking…"];
    case "available":
      return [`Version ${u.version} is ready`, "Downloads, installs and relaunches.", "Update now"];
    case "installing":
      return [
        updateProgress !== null && updateProgress >= 1
          ? `Installing ${u.version}…`
          : `Downloading ${u.version}…`,
        "Tokometer relaunches when it's done.",
        "Updating…",
      ];
    case "up-to-date":
      return ["You're up to date", `Tokometer ${appVersion} is the latest release.`, "Check again"];
    case "failed":
      return [`Tokometer ${appVersion}`, u.reason, "Try again"];
    default:
      return [`Tokometer ${appVersion}`, "", "Check for updates"];
  }
}

function renderNotes(markdown: string) {
  const groups = parseReleaseNotes(markdown);
  updateNotes.replaceChildren();
  updateNotes.hidden = groups.length === 0;
  for (const group of groups) {
    if (group.heading) {
      const heading = document.createElement("h3");
      heading.textContent = group.heading;
      updateNotes.appendChild(heading);
    }
    const list = document.createElement("ul");
    for (const item of group.items) {
      const li = document.createElement("li");
      li.textContent = item;
      list.appendChild(li);
    }
    updateNotes.appendChild(list);
  }
}

function renderUpdate() {
  const u = updatePhase;
  const progress = updateProgress;
  const [title, hint, button] = updateCopy();
  const downloading = u.phase === "installing" && progress !== null && progress < 1;
  updateCard.dataset.phase = u.phase;
  updateCard.classList.toggle("downloading", downloading);
  updateCard.style.setProperty("--progress", downloading ? String(progress) : "0");
  updatePct.textContent = downloading ? `${Math.round(progress * 100)}%` : "";
  updateTitle.textContent = title;
  updateHint.textContent = hint;
  updateHint.hidden = hint === "";
  updateBtn.textContent = button;
  updateBtn.disabled = u.phase === "checking" || u.phase === "installing";
  updateBtn.classList.toggle("primary", u.phase === "available");
  updateDismiss.hidden = !(u.phase === "available" && !u.dismissed);
}

function setUpdatePhase(u: api.UpdatePhase) {
  updatePhase = u;
  updateProgress = null;
  renderNotes(u.phase === "available" ? u.notes : "");
  renderUpdate();
}

void api.onUpdatePhase(setUpdatePhase);
void api.onUpdateProgress((fraction) => {
  updateProgress = fraction;
  renderUpdate();
});

/* ---- dev: P steps the card through every phase locally, with a simulated
 * download, so each state can be seen without a real release. A real phase
 * event takes over again whenever the backend emits one. ---- */
if (import.meta.env.DEV) {
  const PREVIEW: api.UpdatePhase[] = [
    { phase: "checking" },
    {
      phase: "available",
      version: "9.9.9",
      dismissed: false,
      notes: "### Features\n\n* a mock release, for previewing the update card\n* **widget:** nothing real changed\n\n### Bug Fixes\n\n* nothing real was fixed either",
    },
    { phase: "installing", version: "9.9.9" },
    { phase: "up-to-date" },
    { phase: "failed", reason: "Update check failed" },
    { phase: "idle" },
  ];
  let step = -1;
  let download: ReturnType<typeof setInterval> | undefined;
  window.addEventListener("keydown", (e) => {
    if (e.repeat || e.key.toLowerCase() !== "p") return;
    clearInterval(download);
    step = (step + 1) % PREVIEW.length;
    setUpdatePhase(PREVIEW[step]);
    if (PREVIEW[step].phase !== "installing") return;
    let fraction = 0;
    download = setInterval(() => {
      fraction = Math.min(1, fraction + 0.02);
      updateProgress = fraction;
      renderUpdate();
      if (fraction >= 1) clearInterval(download);
    }, 60);
  });
}
