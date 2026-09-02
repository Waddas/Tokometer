import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** One usage limit window, as usage.rs parses it out of the API's `limits`. */
export interface LimitWindow {
  /** Stable id: "session", "weekly_all", or "<kind>:<model-slug>". */
  id: string;
  /** Tile label: "5h", "7d", or the scoped model's name ("Fable"). */
  label: string;
  /** 0-100 percent */
  utilization: number;
  /** unix epoch seconds */
  resetAt: number | null;
  /** A last known value the poll couldn't observe (`carry_missing_windows` in
   *  usage.rs); absent on live windows. */
  stale?: boolean;
}

/** Well-known window ids; mirrors `ID_SESSION`/`ID_WEEKLY_ALL` in usage.rs. */
export const SESSION_ID = "session";
export const WEEKLY_ALL_ID = "weekly_all";

export interface UsageSnapshot {
  status: "ok" | "error";
  source: "oauth" | "messages" | null;
  /** unix epoch ms */
  fetchedAt: number;
  /** Every window the poll reported, in the API's order; empty on failure. */
  windows: LimitWindow[];
  error: string | null;
}

/** One entry of the backend usage-history log (history.rs). */
export interface HistorySample {
  /** unix epoch ms */
  ms: number;
  /** window id → sample; absent ids mean the poll lacked that window */
  w: Record<string, { pct: number; reset?: number | null }>;
}

/** Mirrors the Rust `Layout` enum (state.rs). */
export type Layout =
  | "mascot-left"
  | "mascot-right"
  | "mascot-top"
  | "mascot-bottom"
  | "tiles-row"
  | "tiles-column";

/** Mirrors the Rust `Mascot` enum (state.rs) and `MascotId` (mascots.ts). */
export type Mascot = "clawd" | "axolotl" | "cat";

/** Mirrors the Rust `Size` enum (state.rs). */
export type Size = "small" | "medium" | "large";

/** Mirrors the Rust `TrayStyle` enum (state.rs). */
export type TrayStyle = "ring" | "text";

/** Opt-in feature previews; mirrors the Rust `BetaFeatures` (state.rs). */
export interface BetaFeatures {
  /** Forecast from the learned hour-of-week profile instead of the recent rate. */
  learnedForecast: boolean;
}

/** The persisted preferences, as get_state and state://change report them. */
export interface Preferences {
  pin: boolean;
  layout: Layout;
  size: Size;
  /** Free-resize scale; overrides `size` while set. */
  customScale: number | null;
  mascot: Mascot;
  trayStyle: TrayStyle;
  /** Which weekdays the 7-day prediction ramps, indexed Sun..Sat. */
  workDays: boolean[];
  /** Whether a failing usage endpoint may fall back to the 1-token probe. */
  probeFallback: boolean;
  /** Ids of limit windows the user hid; they get no tile. */
  hiddenLimits: string[];
  beta: BetaFeatures;
}

export interface AppStateSnapshot extends Preferences {
  lastUsage: UsageSnapshot | null;
}

export interface StateChange extends Preferences {
  visible: boolean;
}

export const getState = () => invoke<AppStateSnapshot>("get_state");
export const refreshNow = () => invoke<void>("refresh_now");
export const setPin = (pinned: boolean) => invoke<void>("set_pin", { pinned });
export const setMascot = (mascot: Mascot) => invoke<void>("set_mascot", { mascot });
export const setLayout = (layout: Layout) => invoke<void>("set_layout", { layout });
export const setSize = (size: Size) => invoke<void>("set_size", { size });
export const setTrayStyle = (style: TrayStyle) => invoke<void>("set_tray_style", { style });
export const setWorkDays = (days: boolean[]) => invoke<void>("set_work_days", { days });
export const setHiddenLimits = (ids: string[]) => invoke<void>("set_hidden_limits", { ids });
export const setProbeFallback = (enabled: boolean) =>
  invoke<void>("set_probe_fallback", { enabled });
export const setBeta = (beta: BetaFeatures) => invoke<void>("set_beta", { beta });
/** Size the widget for a logical width, height locked to the layout's aspect
 * ratio; `commit` persists the resulting free-resize scale. */
export const resizeWidget = (width: number, commit: boolean) =>
  invoke<void>("resize_widget", { width, commit });
export const toggleVisibility = () => invoke<void>("toggle_visibility");
export const openSettings = () => invoke<void>("open_settings");
export const getAutostart = () => invoke<boolean>("get_autostart");
export const setAutostart = (enabled: boolean) => invoke<boolean>("set_autostart", { enabled });

export const getHistory = () => invoke<HistorySample[]>("get_history");
/** One-time migration of the pre-backend localStorage history. */
export const importHistory = (samples: HistorySample[]) =>
  invoke<void>("import_history", { samples });

/** Mirrors `UpdatePhase` in update.rs: a check parks a newer release as
 * `available` until the user installs it. */
export type UpdatePhase =
  | { phase: "idle" }
  | { phase: "checking" }
  /** `dismissed`: the user hid this release's dots; it stays installable.
   * `notes`: the release's changelog entry (markdown), empty if it has none. */
  | { phase: "available"; version: string; dismissed: boolean; notes: string }
  | { phase: "installing"; version: string }
  | { phase: "up-to-date" }
  | { phase: "failed"; reason: string };

export const getUpdatePhase = () => invoke<UpdatePhase>("get_update_phase");
export const checkForUpdates = () => invoke<void>("check_for_updates");
/** Download, install and relaunch the release a check found. */
export const installUpdate = () => invoke<void>("install_update");
/** Hide the offered release's dots (widget and tray) until a newer one appears. */
export const dismissUpdate = () => invoke<void>("dismiss_update");

/** Dev/screenshot aid: mirror a mock snapshot in the tray icon (null clears it). */
export const setTrayOverride = (snapshot: UsageSnapshot | null) =>
  invoke<void>("set_tray_override", { snapshot });

/** Dev/screenshot aid: offer a mock release (or withdraw it). */
export const setUpdateOverride = (available: boolean) =>
  invoke<void>("set_update_override", { available });

/** Current update phase now and on every change. */
export const onUpdatePhase = (cb: (u: UpdatePhase) => void): Promise<UnlistenFn> => {
  void getUpdatePhase().then(cb);
  return listen<UpdatePhase>("update://state", (e) => cb(e.payload));
};

/** Download progress of an installing update, 0..1; 1 once the download is
 * done and the installer is running. Silent when the size is unknown. */
export const onUpdateProgress = (cb: (fraction: number) => void): Promise<UnlistenFn> =>
  listen<number>("update://progress", (e) => cb(e.payload));

export const onUsage = (cb: (s: UsageSnapshot) => void): Promise<UnlistenFn> =>
  listen<UsageSnapshot>("usage://update", (e) => cb(e.payload));

export const onStateChange = (cb: (s: StateChange) => void): Promise<UnlistenFn> =>
  listen<StateChange>("state://change", (e) => cb(e.payload));

/** Dev/screenshot aid: tray toggle to hide the dev badge (debug builds only). */
export const onDevBarHidden = (cb: (hidden: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("devbar://hidden", (e) => cb(e.payload));
