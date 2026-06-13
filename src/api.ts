import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface UsageWindow {
  /** 0-100 percent */
  utilization: number;
  /** unix epoch seconds */
  resetAt: number | null;
}

export interface UsageSnapshot {
  status: "ok" | "error";
  source: "oauth" | "messages" | null;
  /** unix epoch ms */
  fetchedAt: number;
  fiveHour: UsageWindow | null;
  sevenDay: UsageWindow | null;
  fiveHourStatus: string | null;
  error: string | null;
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

export interface AppStateSnapshot {
  pin: boolean;
  layout: Layout;
  mascot: Mascot;
  /** Which weekdays the 7-day prediction ramps, indexed Sun..Sat. */
  workDays: boolean[];
  lastUsage: UsageSnapshot | null;
}

export interface StateChange {
  pin: boolean;
  layout: Layout;
  mascot: Mascot;
  /** Which weekdays the 7-day prediction ramps, indexed Sun..Sat. */
  workDays: boolean[];
  visible: boolean;
}

export const getState = () => invoke<AppStateSnapshot>("get_state");
export const refreshNow = () => invoke<void>("refresh_now");
export const setPin = (pinned: boolean) => invoke<void>("set_pin", { pinned });
export const setMascot = (mascot: Mascot) => invoke<void>("set_mascot", { mascot });
export const toggleVisibility = () => invoke<void>("toggle_visibility");

export const onUsage = (cb: (s: UsageSnapshot) => void): Promise<UnlistenFn> =>
  listen<UsageSnapshot>("usage://update", (e) => cb(e.payload));

export const onStateChange = (cb: (s: StateChange) => void): Promise<UnlistenFn> =>
  listen<StateChange>("state://change", (e) => cb(e.payload));

/** Dev/screenshot aid: tray toggle to hide the dev badge (debug builds only). */
export const onDevBarHidden = (cb: (hidden: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>("devbar://hidden", (e) => cb(e.payload));
