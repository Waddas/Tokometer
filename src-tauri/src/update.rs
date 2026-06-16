use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// Delay before the first silent check, so the tray, window and poller settle
/// before a download or restart can interrupt them.
const STARTUP_DELAY: Duration = Duration::from_secs(10);

/// Run one silent check shortly after launch: if a newer GitHub release exists,
/// download, install and relaunch. Stays quiet unless an update actually lands —
/// a failed check must never disturb the widget.
pub fn spawn_startup_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        check_and_install(app, false).await;
    });
}

/// Run an immediate check with user-visible feedback (the "Check for updates"
/// tray item).
pub fn spawn_check(app: AppHandle) {
    tauri::async_runtime::spawn(check_and_install(app, true));
}

/// Check GitHub for a newer release and, if found, install it and relaunch.
/// `manual` surfaces progress and the up-to-date / failure result in the tray
/// status line; the silent startup check leaves it untouched.
async fn check_and_install(app: AppHandle, manual: bool) {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            eprintln!("updater unavailable: {e}");
            if manual {
                set_status(&app, "Update check unavailable");
            }
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            if manual {
                set_status(&app, "Downloading update…");
            }
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    // Persist before the installer relaunches us; restart() never returns.
                    crate::state::save(&app);
                    app.restart();
                }
                Err(e) => {
                    eprintln!("update install failed: {e}");
                    if manual {
                        set_status(&app, "Update failed — try again later");
                    }
                }
            }
        }
        Ok(None) => {
            if manual {
                set_status(&app, "Up to date");
            }
        }
        Err(e) => {
            eprintln!("update check failed: {e}");
            if manual {
                set_status(&app, "Update check failed");
            }
        }
    }
}

/// Show a transient message in the tray status line. The next poll (≤60s)
/// restores the usage figures.
fn set_status(app: &AppHandle, msg: &str) {
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = handles.status_item.set_text(msg);
    }
}
