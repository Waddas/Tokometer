use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::TrayStyle;
use crate::usage::UsageSnapshot;

/// Dev/screenshot aid: a mock snapshot pushed from the frontend (see
/// `set_tray_override`). While set, it overrides the live poll result so the
/// tray matches the previewed widget. Always `None` in release.
pub struct DevOverride(pub Mutex<Option<UsageSnapshot>>);

/// Preferences live in the settings window; the tray keeps only the status
/// line and the actions that make sense without opening anything.
pub struct TrayHandles {
    pub tray: TrayIcon,
    pub status_item: MenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", "Starting…", false, None::<&str>)?;
    let show_hide = MenuItem::with_id(app, "show_hide", "Show / Hide widget", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let check_updates = MenuItem::with_id(
        app,
        "check_updates",
        "Check for updates…",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    // Disabled: the version is information, not an action.
    let version = MenuItem::new(
        app,
        format!("Tokometer {}", app.package_info().version),
        false,
        None::<&str>,
    )?;
    // Dev/screenshot aid: hide the dev badge for clean captures. Debug builds
    // only — release builds never draw the badge (see main.ts).
    let hide_devbar = if cfg!(debug_assertions) {
        Some(CheckMenuItem::with_id(
            app,
            "hide_devbar",
            "Hide dev bar",
            true,
            false,
            None::<&str>,
        )?)
    } else {
        None
    };
    let sep_top = PredefinedMenuItem::separator(app)?;
    let sep_bottom = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&status_item, &sep_top, &show_hide, &settings];
    if let Some(item) = &hide_devbar {
        items.push(item);
    }
    items.extend([
        &sep_bottom as &dyn IsMenuItem<Wry>,
        &refresh,
        &check_updates,
        &version,
        &quit,
    ]);
    let menu = Menu::with_items(app, &items)?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(crate::trayicon::unknown())
        .icon_as_template(true)
        .tooltip("Tokometer — starting…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event({
            let hide_devbar = hide_devbar.clone();
            move |app, event| match event.id().as_ref() {
                "show_hide" => toggle_visibility(app),
                "settings" => crate::commands::show_settings(app),
                "hide_devbar" => {
                    // CheckMenuItem toggles itself before the event fires; read it back.
                    if let Some(item) = &hide_devbar {
                        let _ = app.emit("devbar://hidden", item.is_checked().unwrap_or(false));
                    }
                }
                "refresh" => app.state::<crate::poller::RefreshSignal>().0.notify_one(),
                "check_updates" => crate::update::spawn_check(app.clone()),
                "quit" => {
                    crate::state::save(app);
                    app.exit(0);
                }
                _ => {}
            }
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

    app.manage(TrayHandles { tray, status_item });
    Ok(())
}

pub fn toggle_visibility(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
    } else {
        let _ = win.show();
        let _ = win.set_focus();
    }
    emit_state(app);
}

/// Broadcast the full preference set — the widget and the settings window
/// both re-render from this one event.
pub fn emit_state(app: &AppHandle) {
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let state = app.state::<crate::state::AppState>();
    let payload = {
        let s = state.0.lock().unwrap();
        serde_json::json!({
            "pin": s.pin,
            "layout": s.layout,
            "size": s.size,
            "customScale": s.custom_scale,
            "mascot": s.mascot,
            "trayStyle": s.tray_style,
            "workDays": s.work_days,
            "probeFallback": s.probe_fallback,
            "visible": visible,
        })
    };
    let _ = app.emit("state://change", payload);
}

/// Reflect the latest poll result in the tray: the 5h percentage (or a flat
/// gauge glyph when unknown), tooltip, and status line.
pub fn update(app: &AppHandle, snapshot: &UsageSnapshot) {
    let Some(handles) = app.try_state::<TrayHandles>() else {
        return;
    };

    // A dev override, when set, stands in for the live snapshot (see DevOverride).
    let preview = app
        .try_state::<DevOverride>()
        .and_then(|o| o.0.lock().unwrap().clone());
    let snapshot = preview.as_ref().unwrap_or(snapshot);

    let style = app
        .state::<crate::state::AppState>()
        .0
        .lock()
        .unwrap()
        .tray_style;
    let pct = |w: &Option<crate::usage::UsageWindow>| w.as_ref().map(|w| w.utilization);

    // `template` tints the icon to the macOS menubar (light/dark) — right for the
    // monochrome text and unknown glyphs, but it would strip the ring's colour.
    let (icon, template, line) = if snapshot.status == "ok" {
        let five = pct(&snapshot.five_hour);
        let fmt = |p: Option<f64>| p.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "--".into());
        let line = format!("5h {} • 7d {}", fmt(five), fmt(pct(&snapshot.seven_day)));
        let (icon, template) = match (five, style) {
            (Some(p), TrayStyle::Ring) => (crate::trayicon::gauge(p), false),
            (Some(p), TrayStyle::Text) => (crate::trayicon::text("5h", p), true),
            (None, _) => (crate::trayicon::unknown(), true),
        };
        (icon, template, line)
    } else {
        let err = snapshot.error.clone().unwrap_or_else(|| "error".into());
        (crate::trayicon::unknown(), true, err)
    };

    // Re-assert the template flag each time: plain set_icon would clear it.
    let _ = handles.tray.set_icon_with_as_template(Some(icon), template);
    let _ = handles
        .tray
        .set_tooltip(Some(format!("Tokometer — {line}")));
    let _ = handles.status_item.set_text(&line);
}
