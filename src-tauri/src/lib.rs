mod commands;
mod credentials;
mod poller;
mod state;
mod tray;
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
        .manage(poller::RefreshSignal(Arc::new(Notify::new())))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::refresh_now,
            commands::set_pin,
            commands::set_mascot,
            commands::toggle_visibility,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let persisted = state::load(&handle);
            let pin = persisted.pin;
            let layout = persisted.layout;
            let mascot = persisted.mascot;
            let saved_pos = persisted.window;
            app.manage(state::AppState(Mutex::new(persisted)));

            // Restore position before showing so the window never flashes at the default spot.
            let win = app.get_webview_window("main").expect("main window");
            match saved_pos {
                Some(pos) => {
                    let _ = win.set_position(tauri::LogicalPosition::new(pos.x, pos.y));
                }
                None => {
                    let _ = win.center();
                }
            }
            let _ = win.set_always_on_top(pin);
            let (w, h) = layout.window_size();
            let _ = win.set_size(tauri::LogicalSize::new(w, h));

            tray::create(&handle, pin, layout, mascot)?;
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
