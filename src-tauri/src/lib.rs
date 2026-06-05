mod commands;
mod credentials;
mod poller;
mod state;
mod tray;
mod usage;

use std::sync::{Arc, Mutex};
use tauri::{Manager, RunEvent, WindowEvent};
use tokio::sync::Notify;

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
            commands::toggle_visibility,
            commands::get_autostart,
            commands::set_autostart,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let persisted = state::load(&handle);
            let pin = persisted.pin;
            let layout = persisted.layout;
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

            tray::create(&handle, pin, layout)?;
            let _ = win.show();

            // Track moves in memory (logical coords); flushed to disk on exit and state saves.
            let win_for_scale = win.clone();
            let move_handle = handle.clone();
            win.on_window_event(move |event| {
                if let WindowEvent::Moved(physical) = event {
                    let scale = win_for_scale.scale_factor().unwrap_or(1.0);
                    let logical = physical.to_logical::<f64>(scale);
                    if let Some(state) = move_handle.try_state::<state::AppState>() {
                        state.0.lock().unwrap().window =
                            Some(state::WindowPos { x: logical.x, y: logical.y });
                    }
                }
            });

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
