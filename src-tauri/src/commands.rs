use serde_json::json;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::history::{HistoryLog, Sample};
use crate::poller::RefreshSignal;
use crate::state::{AppState, Layout, Mascot, Size, TrayStyle};

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    json!({
        "pin": s.pin,
        "layout": s.layout,
        "size": s.size,
        "customScale": s.custom_scale,
        "mascot": s.mascot,
        "trayStyle": s.tray_style,
        "workDays": s.work_days,
        "probeFallback": s.probe_fallback,
        "lastUsage": s.last_usage,
    })
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
pub fn set_mascot(app: AppHandle, mascot: String) {
    if let Some(mascot) = Mascot::from_id(&mascot) {
        apply_mascot(&app, mascot);
    }
}

#[tauri::command]
pub fn set_layout(app: AppHandle, layout: String) {
    if let Some(layout) = Layout::from_id(&layout) {
        apply_layout(&app, layout);
    }
}

#[tauri::command]
pub fn set_size(app: AppHandle, size: String) {
    if let Some(size) = Size::from_id(&size) {
        apply_size(&app, size);
    }
}

#[tauri::command]
pub fn set_tray_style(app: AppHandle, style: String) {
    if let Some(style) = TrayStyle::from_id(&style) {
        apply_tray_style(&app, style);
    }
}

#[tauri::command]
pub fn set_work_days(app: AppHandle, days: Vec<bool>) {
    let Ok(days) = <[bool; 7]>::try_from(days) else {
        return;
    };
    app.state::<AppState>().0.lock().unwrap().work_days = days;
    crate::state::save(&app);
    crate::tray::emit_state(&app);
}

#[tauri::command]
pub fn set_probe_fallback(app: AppHandle, enabled: bool) {
    app.state::<AppState>().0.lock().unwrap().probe_fallback = enabled;
    crate::state::save(&app);
    crate::tray::emit_state(&app);
}

#[tauri::command]
pub fn toggle_visibility(app: AppHandle) {
    crate::tray::toggle_visibility(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings(&app);
}

#[tauri::command]
pub fn get_history(log: State<'_, HistoryLog>) -> Vec<Sample> {
    log.0.lock().unwrap().clone()
}

/// One-time migration of the pre-backend localStorage history (see
/// `history::import` for the merge rules).
#[tauri::command]
pub fn import_history(app: AppHandle, samples: Vec<Sample>) {
    {
        let log = app.state::<HistoryLog>();
        let mut log = log.0.lock().unwrap();
        crate::history::import(&mut log, samples, crate::usage::now_ms());
    }
    crate::history::save(&app);
}

/// Dev/screenshot aid: mirror a mock snapshot in the tray (or clear it with
/// `None`). The override wins over live polls until cleared, so the tray tracks
/// the previewed widget. Release builds never invoke this (the mock UI is
/// dev-only), so the override stays `None`.
#[tauri::command]
pub fn set_tray_override(app: AppHandle, snapshot: Option<crate::usage::UsageSnapshot>) {
    *app.state::<crate::tray::DevOverride>().0.lock().unwrap() = snapshot;
    // Re-render now. update() re-applies the override; fall back to the last live
    // snapshot so clearing the override restores the real figure immediately.
    let render = {
        let ov = app
            .state::<crate::tray::DevOverride>()
            .0
            .lock()
            .unwrap()
            .clone();
        ov.or_else(|| app.state::<AppState>().0.lock().unwrap().last_usage.clone())
    };
    if let Some(snapshot) = render {
        crate::tray::update(&app, &snapshot);
    }
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> bool {
    let autolaunch = app.autolaunch();
    let _ = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    autolaunch.is_enabled().unwrap_or(false)
}

/// Show the settings window, creating it on first use. Also the tray's
/// "Settings…" entry point.
pub fn show_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let _ = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Tokometer Settings")
    .inner_size(420.0, 620.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    // Born hidden, and black underneath: the page shows itself once its
    // first render has landed (settings.ts), but the webview can still
    // surface before its first paint — a native background in the page's
    // colour keeps that moment invisible instead of a white flash.
    .visible(false)
    .background_color(tauri::window::Color(0, 0, 0, 255))
    .build();
}

fn resize_main(app: &AppHandle, layout: Layout, scale: f64) {
    if let Some(win) = app.get_webview_window("main") {
        let (w, h) = layout.window_size(scale);
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
    }
}

/// Single mutation path for "pin on top" — used by the UI command and settings.
pub fn apply_pin(app: &AppHandle, pinned: bool) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_always_on_top(pinned);
    }
    app.state::<AppState>().0.lock().unwrap().pin = pinned;
    crate::state::save(app);
    crate::tray::emit_state(app);
}

/// Single mutation path for the widget layout — resizes the window and persists.
pub fn apply_layout(app: &AppHandle, layout: Layout) {
    let scale = {
        let state = app.state::<AppState>();
        let mut s = state.0.lock().unwrap();
        s.layout = layout;
        s.effective_scale()
    };
    resize_main(app, layout, scale);
    crate::state::save(app);
    crate::tray::emit_state(app);
}

/// Single mutation path for a size preset — clears any free-resize scale,
/// resizes the window for the current layout, and persists.
pub fn apply_size(app: &AppHandle, size: Size) {
    let (layout, scale) = {
        let state = app.state::<AppState>();
        let mut s = state.0.lock().unwrap();
        s.size = size;
        s.custom_scale = None;
        (s.layout, s.effective_scale())
    };
    resize_main(app, layout, scale);
    crate::state::save(app);
    crate::tray::emit_state(app);
}

/// Single mutation path for the mascot — persists; the splash swaps artwork
/// on the state change event.
pub fn apply_mascot(app: &AppHandle, mascot: Mascot) {
    app.state::<AppState>().0.lock().unwrap().mascot = mascot;
    crate::state::save(app);
    crate::tray::emit_state(app);
}

/// Single mutation path for the tray icon style — persists and re-renders the
/// icon from the last poll result.
pub fn apply_tray_style(app: &AppHandle, style: TrayStyle) {
    let snapshot = {
        let state = app.state::<AppState>();
        let mut s = state.0.lock().unwrap();
        s.tray_style = style;
        s.last_usage.clone()
    };
    crate::state::save(app);
    crate::tray::emit_state(app);
    if let Some(snapshot) = snapshot {
        crate::tray::update(app, &snapshot);
    }
}

/// One step of the corner-grip drag (see main.ts): size the window for the
/// requested logical width with the height locked to the layout's aspect
/// ratio, so no drag can distort the widget. `commit` (the drag ending)
/// persists the resulting free-resize scale.
#[tauri::command]
pub fn resize_widget(app: AppHandle, width: f64, commit: bool) {
    if !width.is_finite() {
        return;
    }
    let layout = app.state::<AppState>().0.lock().unwrap().layout;
    let scale = layout.scale_for_width(width);
    resize_main(&app, layout, scale);
    if commit {
        app.state::<AppState>().0.lock().unwrap().custom_scale = Some(scale);
        crate::state::save(&app);
        crate::tray::emit_state(&app);
    }
}
