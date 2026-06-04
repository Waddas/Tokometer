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

export interface AppStateSnapshot {
  pin: boolean;
  lastUsage: UsageSnapshot | null;
}

export const getState = () => invoke<AppStateSnapshot>("get_state");
export const refreshNow = () => invoke<void>("refresh_now");
export const setPin = (pinned: boolean) => invoke<void>("set_pin", { pinned });
export const toggleVisibility = () => invoke<void>("toggle_visibility");

export const onUsage = (cb: (s: UsageSnapshot) => void): Promise<UnlistenFn> =>
  listen<UsageSnapshot>("usage://update", (e) => cb(e.payload));

export const onStateChange = (cb: (s: { pin: boolean; visible: boolean }) => void): Promise<UnlistenFn> =>
  listen<{ pin: boolean; visible: boolean }>("state://change", (e) => cb(e.payload));
