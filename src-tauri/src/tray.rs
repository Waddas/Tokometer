use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::usage::UsageSnapshot;

pub struct TrayHandles {
    pub tray: TrayIcon,
    pub status_item: MenuItem<Wry>,
    pub pin_item: CheckMenuItem<Wry>,
    pub compact_item: CheckMenuItem<Wry>,
    pub autostart_item: CheckMenuItem<Wry>,
}

#[derive(Clone, Copy)]
pub enum TrayStatus {
    Ok,
    Busy,
    Error,
}

fn icon(status: TrayStatus) -> Image<'static> {
    let bytes: &[u8] = match status {
        TrayStatus::Ok => include_bytes!("../icons/tray-ok.png"),
        TrayStatus::Busy => include_bytes!("../icons/tray-busy.png"),
        TrayStatus::Error => include_bytes!("../icons/tray-error.png"),
    };
    Image::from_bytes(bytes).expect("embedded tray icon is valid png")
}

pub fn create(app: &AppHandle, pinned: bool, compact: bool) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", "Starting…", false, None::<&str>)?;
    let show_hide = MenuItem::with_id(app, "show_hide", "Show / Hide widget", true, None::<&str>)?;
    let pin_item = CheckMenuItem::with_id(app, "pin", "Pin on top", true, pinned, None::<&str>)?;
    let compact_item =
        CheckMenuItem::with_id(app, "compact", "Compact view", true, compact, None::<&str>)?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart_item =
        CheckMenuItem::with_id(app, "autostart", "Start at login", true, autostart_on, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &PredefinedMenuItem::separator(app)?,
            &show_hide,
            &pin_item,
            &compact_item,
            &autostart_item,
            &PredefinedMenuItem::separator(app)?,
            &refresh,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(TrayStatus::Busy))
        .tooltip("clordgauge — starting…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide" => toggle_visibility(app),
            "pin" => {
                // CheckMenuItem toggles itself before the event fires.
                let checked = app
                    .state::<TrayHandles>()
                    .pin_item
                    .is_checked()
                    .unwrap_or(false);
                crate::commands::apply_pin(app, checked);
            }
            "compact" => {
                let checked = app
                    .state::<TrayHandles>()
                    .compact_item
                    .is_checked()
                    .unwrap_or(false);
                crate::commands::apply_compact(app, checked);
            }
            "autostart" => {
                let autolaunch = app.autolaunch();
                let enable = !autolaunch.is_enabled().unwrap_or(false);
                let _ = if enable { autolaunch.enable() } else { autolaunch.disable() };
                let _ = app
                    .state::<TrayHandles>()
                    .autostart_item
                    .set_checked(autolaunch.is_enabled().unwrap_or(false));
            }
            "refresh" => app.state::<crate::poller::RefreshSignal>().0.notify_one(),
            "quit" => {
                crate::state::save(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_visibility(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayHandles { tray, status_item, pin_item, compact_item, autostart_item });
    Ok(())
}

pub fn toggle_visibility(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
    }
    emit_state(app);
}

pub fn emit_state(app: &AppHandle) {
    let state = app.state::<crate::state::AppState>();
    let (pin, compact) = {
        let s = state.0.lock().unwrap();
        (s.pin, s.compact)
    };
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let _ = app.emit(
        "state://change",
        serde_json::json!({ "pin": pin, "compact": compact, "visible": visible }),
    );
}

/// Reflect the latest poll result in the tray: bubble color, tooltip, status line.
pub fn update(app: &AppHandle, snapshot: &UsageSnapshot) {
    let Some(handles) = app.try_state::<TrayHandles>() else { return };
    let (status, line) = if snapshot.status == "ok" {
        let pct = |w: &Option<crate::usage::UsageWindow>| {
            w.as_ref().map(|w| w.utilization).unwrap_or(0.0)
        };
        (
            TrayStatus::Ok,
            format!("5h {:.0}% • 7d {:.0}%", pct(&snapshot.five_hour), pct(&snapshot.seven_day)),
        )
    } else {
        (
            TrayStatus::Error,
            snapshot.error.clone().unwrap_or_else(|| "error".into()),
        )
    };
    let _ = handles.tray.set_icon(Some(icon(status)));
    let _ = handles.tray.set_tooltip(Some(format!("clordgauge — {line}")));
    let _ = handles.status_item.set_text(&line);
}
