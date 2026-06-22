mod commands;
mod credentials;
mod poller;
mod state;
mod tray;
mod trayicon;
mod update;
mod usage;

use std::sync::{Arc, Mutex};
use tauri::{Manager, RunEvent, WindowEvent};
use tokio::sync::Notify;

/// Raise the window back to the top of the topmost z-band (above the taskbar).
/// tao no-ops `set_always_on_top(true)` when the flag is already set, so this
/// issues the SetWindowPos call directly.
#[cfg(target_os = "windows")]
fn raise_topmost(win: &tauri::WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    if let Ok(hwnd) = win.hwnd() {
        unsafe {
            SetWindowPos(
                hwnd.0 as _,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

/// Whether the mouse cursor is currently over the widget window.
#[cfg(target_os = "windows")]
fn cursor_over_window(win: &tauri::WebviewWindow) -> bool {
    let (Ok(pos), Ok(size), Ok(cursor)) =
        (win.outer_position(), win.outer_size(), win.cursor_position())
    else {
        return false;
    };
    cursor.x >= pos.x as f64
        && cursor.x < (pos.x + size.width as i32) as f64
        && cursor.y >= pos.y as f64
        && cursor.y < (pos.y + size.height as i32) as f64
}

/// A rectangle in physical screen pixels.
#[derive(Clone, Copy)]
struct ScreenRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl ScreenRect {
    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    fn intersects(self, other: ScreenRect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    fn center(self) -> (i64, i64) {
        (self.x as i64 + self.w as i64 / 2, self.y as i64 + self.h as i64 / 2)
    }
}

/// Where to place a window so it sits fully inside the nearest monitor's work
/// area — but only when its current position overlaps none of them. Returns the
/// clamped top-left in physical pixels, or `None` when the window is still
/// (partly) visible and should stay where the user put it.
fn fit_to_work_area(win: ScreenRect, work_areas: &[ScreenRect]) -> Option<(i32, i32)> {
    if work_areas.iter().any(|a| a.intersects(win)) {
        return None;
    }
    let (wx, wy) = win.center();
    let nearest = work_areas.iter().min_by_key(|a| {
        let (ax, ay) = a.center();
        (ax - wx).pow(2) + (ay - wy).pow(2)
    })?;
    // Clamp the whole window inside the area, pinning it to the top-left corner
    // if it is larger than the area in either axis.
    let x = win.x.min(nearest.right() - win.w).max(nearest.x);
    let y = win.y.min(nearest.bottom() - win.h).max(nearest.y);
    Some((x, y))
}

/// Snap the widget into the nearest monitor's work area when its restored
/// position has stranded it off-screen — the display it was saved on may have
/// been unplugged or the monitor layout may have changed. The saved position is
/// left untouched so the widget returns to its original spot if that monitor
/// comes back.
fn ensure_on_screen(win: &tauri::WebviewWindow) {
    let (Ok(pos), Ok(size), Ok(monitors)) =
        (win.outer_position(), win.outer_size(), win.available_monitors())
    else {
        return;
    };
    let work_areas: Vec<ScreenRect> = monitors
        .iter()
        .map(|m| {
            let a = m.work_area();
            ScreenRect {
                x: a.position.x,
                y: a.position.y,
                w: a.size.width as i32,
                h: a.size.height as i32,
            }
        })
        .collect();
    let win_rect = ScreenRect { x: pos.x, y: pos.y, w: size.width as i32, h: size.height as i32 };
    if let Some((x, y)) = fit_to_work_area(win_rect, &work_areas) {
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Second launch: reveal the existing widget instead.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(poller::RefreshSignal(Arc::new(Notify::new())))
        .manage(tray::DevOverride(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::refresh_now,
            commands::set_pin,
            commands::set_mascot,
            commands::toggle_visibility,
            commands::set_tray_override,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .setup(|app| {
            // Tray-only app: keep it out of the macOS Dock (matches Windows'
            // skipTaskbar). Accessory apps have no Dock icon or menu bar but
            // can still show windows.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let persisted = state::load(&handle);
            let pin = persisted.pin;
            let layout = persisted.layout;
            let size = persisted.size;
            let mascot = persisted.mascot;
            let tray_style = persisted.tray_style;
            let work_days = persisted.work_days;
            let saved_pos = persisted.window;
            app.manage(state::AppState(Mutex::new(persisted)));

            // Restore size before position so the off-screen guard measures the
            // real window bounds; do both before showing so the window never
            // flashes at the default spot.
            let win = app.get_webview_window("main").expect("main window");
            let (w, h) = layout.window_size(size);
            let _ = win.set_size(tauri::LogicalSize::new(w, h));
            match saved_pos {
                Some(pos) => {
                    let _ = win.set_position(tauri::LogicalPosition::new(pos.x, pos.y));
                    ensure_on_screen(&win);
                }
                None => {
                    let _ = win.center();
                }
            }
            let _ = win.set_always_on_top(pin);

            tray::create(&handle, pin, layout, size, mascot, tray_style, work_days)?;
            let _ = win.show();

            let event_win = win.clone();
            let event_handle = handle.clone();
            win.on_window_event(move |event| match event {
                // Track moves in memory (logical coords); flushed to disk on exit and state saves.
                WindowEvent::Moved(physical) => {
                    let scale = event_win.scale_factor().unwrap_or(1.0);
                    let logical = physical.to_logical::<f64>(scale);
                    if let Some(state) = event_handle.try_state::<state::AppState>() {
                        state.0.lock().unwrap().window =
                            Some(state::WindowPos { x: logical.x, y: logical.y });
                    }
                }
                // The Windows taskbar shares the topmost z-band with a pinned widget
                // and raises itself above us when clicked — re-assert right away.
                #[cfg(target_os = "windows")]
                WindowEvent::Focused(false) => {
                    if let Some(state) = event_handle.try_state::<state::AppState>() {
                        if state.0.lock().unwrap().pin {
                            raise_topmost(&event_win);
                        }
                    }
                }
                _ => {}
            });

            // Catch taskbar raises that happen while the widget isn't focused
            // (no event reaches us) by periodically re-asserting topmost.
            #[cfg(target_os = "windows")]
            {
                let topmost_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                    loop {
                        tick.tick().await;
                        if !topmost_handle.state::<state::AppState>().0.lock().unwrap().pin {
                            continue;
                        }
                        if let Some(win) = topmost_handle.get_webview_window("main") {
                            // Skip while hovered: the raise would hide native
                            // tooltips, and the user is on the widget anyway.
                            if win.is_visible().unwrap_or(false) && !cursor_over_window(&win) {
                                raise_topmost(&win);
                            }
                        }
                    }
                });
            }

            poller::spawn(handle);
            update::spawn_startup_check(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            state::save(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{fit_to_work_area, ScreenRect};

    fn rect(x: i32, y: i32, w: i32, h: i32) -> ScreenRect {
        ScreenRect { x, y, w, h }
    }

    #[test]
    fn leaves_a_window_that_overlaps_a_work_area() {
        let areas = [rect(0, 0, 2560, 1440)];
        // Fully inside.
        assert_eq!(fit_to_work_area(rect(100, 100, 188, 140), &areas), None);
        // Straddling an edge still counts as visible.
        assert_eq!(fit_to_work_area(rect(-50, 100, 188, 140), &areas), None);
    }

    #[test]
    fn snaps_a_fully_offscreen_window_to_the_work_area_edge() {
        let areas = [rect(0, 0, 2560, 1440)];
        // Off the left edge: clamp x to the left, keep y.
        assert_eq!(fit_to_work_area(rect(-300, 200, 188, 140), &areas), Some((0, 200)));
        // Off the bottom: clamp y to (bottom - height), keep x.
        assert_eq!(fit_to_work_area(rect(200, 1500, 188, 140), &areas), Some((200, 1300)));
    }

    #[test]
    fn picks_the_nearest_monitor_when_several_are_connected() {
        // Regression: a widget saved on a now-unplugged left monitor (negative x)
        // must land on the primary monitor it is closest to, not the far one.
        let primary = rect(0, 24, 2560, 1368);
        let right = rect(2560, 24, 2560, 1368);
        assert_eq!(fit_to_work_area(rect(-245, 688, 188, 140), &[primary, right]), Some((0, 688)));
    }

    #[test]
    fn pins_to_the_corner_when_the_window_is_larger_than_the_area() {
        let areas = [rect(0, 0, 100, 100)];
        assert_eq!(fit_to_work_area(rect(-500, -500, 188, 140), &areas), Some((0, 0)));
    }

    #[test]
    fn returns_none_when_no_monitors_are_available() {
        assert_eq!(fit_to_work_area(rect(-300, 200, 188, 140), &[]), None);
    }
}
