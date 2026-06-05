use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::poller::RefreshSignal;
use crate::state::{AppState, Layout};

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    json!({ "pin": s.pin, "layout": s.layout, "lastUsage": s.last_usage })
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

/// Single mutation path for the widget layout — resizes the window, persists,
/// and syncs the tray's radio-style check items.
pub fn apply_layout(app: &AppHandle, layout: Layout) {
    if let Some(win) = app.get_webview_window("main") {
        let (w, h) = layout.window_size();
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
    app.state::<AppState>().0.lock().unwrap().layout = layout;
    crate::state::save(app);
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        for (l, item) in &handles.layout_items {
            let _ = item.set_checked(*l == layout);
        }
    }
    crate::tray::emit_state(app);
}
