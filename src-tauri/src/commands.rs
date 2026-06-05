use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::poller::RefreshSignal;
use crate::state::AppState;

/// Logical window sizes for the two view modes.
pub const FULL_SIZE: (f64, f64) = (320.0, 320.0);
pub const COMPACT_SIZE: (f64, f64) = (188.0, 112.0);

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    json!({ "pin": s.pin, "compact": s.compact, "lastUsage": s.last_usage })
}

#[tauri::command]
pub fn refresh_now(signal: State<'_, RefreshSignal>) {
    signal.0.notify_one();
}

#[tauri::command]
pub fn set_pin(app: AppHandle, pinned: bool) {
    apply_pin(&app, pinned);
}

#[tauri::command]
pub fn set_compact(app: AppHandle, compact: bool) {
    apply_compact(&app, compact);
}

#[tauri::command]
pub fn toggle_visibility(app: AppHandle) {
    crate::tray::toggle_visibility(&app);
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> bool {
    let autolaunch = app.autolaunch();
    let _ = if enabled { autolaunch.enable() } else { autolaunch.disable() };
    let now = autolaunch.is_enabled().unwrap_or(false);
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = handles.autostart_item.set_checked(now);
    }
    now
}

/// Single mutation path for "pin on top" — used by both the UI command and the tray.
pub fn apply_pin(app: &AppHandle, pinned: bool) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_always_on_top(pinned);
    }
    app.state::<AppState>().0.lock().unwrap().pin = pinned;
    crate::state::save(app);
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = handles.pin_item.set_checked(pinned);
    }
    crate::tray::emit_state(app);
}

/// Single mutation path for compact view — used by both the UI command and the tray.
pub fn apply_compact(app: &AppHandle, compact: bool) {
    if let Some(win) = app.get_webview_window("main") {
        let (w, h) = if compact { COMPACT_SIZE } else { FULL_SIZE };
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    app.state::<AppState>().0.lock().unwrap().compact = compact;
    crate::state::save(app);
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = handles.compact_item.set_checked(compact);
    }
    crate::tray::emit_state(app);
}
