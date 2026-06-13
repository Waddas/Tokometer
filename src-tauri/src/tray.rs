use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::state::{Layout, Mascot, Size};
use crate::usage::UsageSnapshot;

pub struct TrayHandles {
    pub tray: TrayIcon,
    pub status_item: MenuItem<Wry>,
    pub pin_item: CheckMenuItem<Wry>,
    /// Radio-style: exactly one is checked; apply_layout keeps them in sync.
    pub layout_items: Vec<(Layout, CheckMenuItem<Wry>)>,
    /// Radio-style: exactly one is checked; apply_size keeps them in sync.
    pub size_items: Vec<(Size, CheckMenuItem<Wry>)>,
    /// Radio-style: exactly one is checked; apply_mascot keeps them in sync.
    pub mascot_items: Vec<(Mascot, CheckMenuItem<Wry>)>,
    /// Independent checkboxes, keyed by Sun..Sat index (0..6).
    pub work_day_items: Vec<(usize, CheckMenuItem<Wry>)>,
    pub autostart_item: CheckMenuItem<Wry>,
}

/// Work-day submenu order: shown Monday-first, but each maps to its Sun..Sat
/// index (0..6) to match the frontend's `Date.getDay()`.
const WORK_DAYS: [(usize, &str); 7] = [
    (1, "Monday"),
    (2, "Tuesday"),
    (3, "Wednesday"),
    (4, "Thursday"),
    (5, "Friday"),
    (6, "Saturday"),
    (0, "Sunday"),
];

fn layout_label(layout: Layout) -> &'static str {
    match layout {
        Layout::MascotLeft => "Display left",
        Layout::MascotRight => "Display right",
        Layout::MascotTop => "Display top",
        Layout::MascotBottom => "Display bottom",
        Layout::TilesRow => "Tiles only (wide)",
        Layout::TilesColumn => "Tiles only (tall)",
    }
}

fn size_label(size: Size) -> &'static str {
    match size {
        Size::Small => "Small",
        Size::Medium => "Medium",
        Size::Large => "Large",
    }
}

fn mascot_label(mascot: Mascot) -> &'static str {
    match mascot {
        Mascot::Clawd => "Clawd",
        Mascot::Axolotl => "Axolotl",
        Mascot::Cat => "Cat",
    }
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

pub fn create(
    app: &AppHandle,
    pinned: bool,
    layout: Layout,
    size: Size,
    mascot: Mascot,
    work_days: [bool; 7],
) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", "Starting…", false, None::<&str>)?;
    let show_hide = MenuItem::with_id(app, "show_hide", "Show / Hide widget", true, None::<&str>)?;
    let pin_item = CheckMenuItem::with_id(app, "pin", "Pin on top", true, pinned, None::<&str>)?;
    let layout_items: Vec<(Layout, CheckMenuItem<Wry>)> = Layout::ALL
        .into_iter()
        .map(|l| {
            let item = CheckMenuItem::with_id(
                app,
                format!("layout:{}", l.id()),
                layout_label(l),
                true,
                l == layout,
                None::<&str>,
            )?;
            Ok((l, item))
        })
        .collect::<tauri::Result<_>>()?;
    let layout_refs: Vec<&dyn IsMenuItem<Wry>> =
        layout_items.iter().map(|(_, item)| item as &dyn IsMenuItem<Wry>).collect();
    let layout_menu = Submenu::with_items(app, "Layout", true, &layout_refs)?;
    let size_items: Vec<(Size, CheckMenuItem<Wry>)> = Size::ALL
        .into_iter()
        .map(|s| {
            let item = CheckMenuItem::with_id(
                app,
                format!("size:{}", s.id()),
                size_label(s),
                true,
                s == size,
                None::<&str>,
            )?;
            Ok((s, item))
        })
        .collect::<tauri::Result<_>>()?;
    let size_refs: Vec<&dyn IsMenuItem<Wry>> =
        size_items.iter().map(|(_, item)| item as &dyn IsMenuItem<Wry>).collect();
    let size_menu = Submenu::with_items(app, "Size", true, &size_refs)?;
    let mascot_items: Vec<(Mascot, CheckMenuItem<Wry>)> = Mascot::ALL
        .into_iter()
        .map(|m| {
            let item = CheckMenuItem::with_id(
                app,
                format!("mascot:{}", m.id()),
                mascot_label(m),
                true,
                m == mascot,
                None::<&str>,
            )?;
            Ok((m, item))
        })
        .collect::<tauri::Result<_>>()?;
    let mascot_refs: Vec<&dyn IsMenuItem<Wry>> =
        mascot_items.iter().map(|(_, item)| item as &dyn IsMenuItem<Wry>).collect();
    let mascot_menu = Submenu::with_items(app, "Mascot", true, &mascot_refs)?;
    // Independent checkboxes (not radio): which days the 7-day prediction ramps.
    let work_day_items: Vec<(usize, CheckMenuItem<Wry>)> = WORK_DAYS
        .into_iter()
        .map(|(day, label)| {
            let item = CheckMenuItem::with_id(
                app,
                format!("workday:{day}"),
                label,
                true,
                work_days[day],
                None::<&str>,
            )?;
            Ok((day, item))
        })
        .collect::<tauri::Result<_>>()?;
    let work_day_refs: Vec<&dyn IsMenuItem<Wry>> =
        work_day_items.iter().map(|(_, item)| item as &dyn IsMenuItem<Wry>).collect();
    let work_days_menu = Submenu::with_items(app, "Work days", true, &work_day_refs)?;
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart_item =
        CheckMenuItem::with_id(app, "autostart", "Start at login", true, autostart_on, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    // Dev/screenshot aid: hide the dev badge for clean captures. Debug builds
    // only — release builds never draw the badge (see main.ts).
    let hide_devbar = if cfg!(debug_assertions) {
        Some(CheckMenuItem::with_id(app, "hide_devbar", "Hide dev bar", true, false, None::<&str>)?)
    } else {
        None
    };
    let sep_top = PredefinedMenuItem::separator(app)?;
    let sep_bottom = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![
        &status_item,
        &sep_top,
        &show_hide,
        &layout_menu,
        &size_menu,
        &mascot_menu,
        &work_days_menu,
        &pin_item,
        &autostart_item,
    ];
    if let Some(item) = &hide_devbar {
        items.push(item);
    }
    items.extend([&sep_bottom as &dyn IsMenuItem<Wry>, &refresh, &quit]);
    let menu = Menu::with_items(app, &items)?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(TrayStatus::Busy))
        .tooltip("clordgauge — starting…")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event({
            let hide_devbar = hide_devbar.clone();
            move |app, event| match event.id().as_ref() {
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
            id if id.starts_with("layout:") => {
                if let Some(layout) = Layout::from_id(&id["layout:".len()..]) {
                    crate::commands::apply_layout(app, layout);
                }
            }
            id if id.starts_with("size:") => {
                if let Some(size) = Size::from_id(&id["size:".len()..]) {
                    crate::commands::apply_size(app, size);
                }
            }
            id if id.starts_with("mascot:") => {
                if let Some(mascot) = Mascot::from_id(&id["mascot:".len()..]) {
                    crate::commands::apply_mascot(app, mascot);
                }
            }
            id if id.starts_with("workday:") => {
                // CheckMenuItem toggles itself before the event fires; read it back.
                if let Ok(day) = id["workday:".len()..].parse::<usize>() {
                    let on = app
                        .state::<TrayHandles>()
                        .work_day_items
                        .iter()
                        .find(|(d, _)| *d == day)
                        .map(|(_, item)| item.is_checked().unwrap_or(true))
                        .unwrap_or(true);
                    crate::commands::apply_work_day(app, day, on);
                }
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
            "hide_devbar" => {
                // CheckMenuItem toggles itself before the event fires; read it back.
                if let Some(item) = &hide_devbar {
                    let _ = app.emit("devbar://hidden", item.is_checked().unwrap_or(false));
                }
            }
            "refresh" => app.state::<crate::poller::RefreshSignal>().0.notify_one(),
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

    app.manage(TrayHandles {
        tray,
        status_item,
        pin_item,
        layout_items,
        size_items,
        mascot_items,
        work_day_items,
        autostart_item,
    });
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
    let (pin, layout, mascot, work_days) = {
        let s = state.0.lock().unwrap();
        (s.pin, s.layout, s.mascot, s.work_days)
    };
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let _ = app.emit(
        "state://change",
        serde_json::json!({
            "pin": pin,
            "layout": layout,
            "mascot": mascot,
            "workDays": work_days,
            "visible": visible,
        }),
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
