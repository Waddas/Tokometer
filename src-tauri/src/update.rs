//! Self-update flow: check → offer → install. A check never installs on its
//! own; it parks the release as `Available` and the UI (tray item and badge,
//! settings button, widget dot) offers it until the user chooses to update.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Delay before the silent startup check, so the tray, window and poller
/// settle before the network request goes out.
const STARTUP_DELAY: Duration = Duration::from_secs(10);

/// Version and notes the dev override advertises (see `set_override`).
const MOCK_VERSION: &str = "9.9.9";
const MOCK_NOTES: &str = "### Features\n\n* a mock release, for previewing the update card ([#0](x)) ([0000000](x))\n* **widget:** nothing real changed\n\n### Bug Fixes\n\n* nothing real was fixed either\n";

/// Where the updater is; mirrored to the frontend as `update://state` and
/// `UpdatePhase` in api.ts. `UpToDate` and `Failed` are the results of a
/// manual check, cleared by the next one.
#[derive(Clone, Default, Serialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    /// `dismissed`: the user hid this release's dots; it stays installable.
    /// `notes`: the release's changelog entry (markdown), empty if none.
    Available {
        version: String,
        dismissed: bool,
        notes: String,
    },
    Installing {
        version: String,
    },
    UpToDate,
    Failed {
        reason: String,
    },
}

impl UpdatePhase {
    /// The tray item is the one action that fits the phase: check, or install
    /// the offered release; disabled while either is in flight.
    fn tray_item(&self) -> (String, bool) {
        match self {
            Self::Checking => ("Checking for updates…".into(), false),
            Self::Available { version, .. } => (format!("Update to {version}…"), true),
            Self::Installing { .. } => ("Installing update…".into(), false),
            _ => ("Check for updates…".into(), true),
        }
    }

    /// Whether the tray icon and widget show the update-available dot.
    fn offers_dot(&self) -> bool {
        matches!(
            self,
            Self::Available {
                dismissed: false,
                ..
            }
        )
    }
}

#[derive(Default)]
struct Inner {
    phase: UpdatePhase,
    /// The release behind `Available`, kept so install needs no second check.
    pending: Option<Update>,
}

#[derive(Default)]
pub struct UpdateState(Mutex<Inner>);

pub fn phase(app: &AppHandle) -> UpdatePhase {
    app.state::<UpdateState>().0.lock().unwrap().phase.clone()
}

pub fn badge_wanted(app: &AppHandle) -> bool {
    phase(app).offers_dot()
}

fn set_phase(app: &AppHandle, phase: UpdatePhase, pending: Option<Update>) {
    {
        let state = app.state::<UpdateState>();
        let mut inner = state.0.lock().unwrap();
        inner.phase = phase.clone();
        inner.pending = pending;
    }
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let (text, enabled) = phase.tray_item();
        let _ = handles.update_item.set_text(text);
        let _ = handles.update_item.set_enabled(enabled);
    }
    crate::tray::set_dismiss_offered(app, phase.offers_dot());
    let _ = app.emit("update://state", &phase);
    crate::tray::refresh(app);
}

/// `Available` for `version`, honouring an earlier dismissal of that release.
fn available(app: &AppHandle, version: String, notes: String) -> UpdatePhase {
    let dismissed = app
        .state::<crate::state::AppState>()
        .0
        .lock()
        .unwrap()
        .dismissed_update
        .as_deref()
        == Some(version.as_str());
    UpdatePhase::Available {
        version,
        dismissed,
        notes,
    }
}

/// Run one silent check shortly after launch. Only an available release
/// changes anything visible; a failed or empty check leaves the app untouched.
pub fn spawn_startup_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        check(app, false).await;
    });
}

/// The tray item: check, or install the release a check already found.
pub fn activate(app: &AppHandle) {
    match phase(app) {
        UpdatePhase::Available { .. } => spawn_install(app.clone()),
        _ => spawn_check(app.clone()),
    }
}

pub fn spawn_check(app: AppHandle) {
    tauri::async_runtime::spawn(check(app, true));
}

pub fn spawn_install(app: AppHandle) {
    tauri::async_runtime::spawn(install(app));
}

/// Hide the offered release's dots until a newer release appears.
pub fn dismiss(app: &AppHandle) {
    let UpdatePhase::Available { version, notes, .. } = phase(app) else {
        return;
    };
    let pending = app.state::<UpdateState>().0.lock().unwrap().pending.clone();
    app.state::<crate::state::AppState>()
        .0
        .lock()
        .unwrap()
        .dismissed_update = Some(version.clone());
    crate::state::save(app);
    set_phase(
        app,
        UpdatePhase::Available {
            version,
            dismissed: true,
            notes,
        },
        pending,
    );
}

/// Dev/screenshot aid: pretend a release is (or is no longer) available so the
/// offer UI can be previewed. Debug builds only; installing the mock reports
/// the usual dev refusal. Offering the mock forgets an earlier dismissal of it
/// so the dots can be previewed again.
pub fn set_override(app: &AppHandle, available: bool) {
    if !cfg!(debug_assertions) {
        return;
    }
    let phase = if available {
        let state = app.state::<crate::state::AppState>();
        let mut s = state.0.lock().unwrap();
        if s.dismissed_update.as_deref() == Some(MOCK_VERSION) {
            s.dismissed_update = None;
        }
        UpdatePhase::Available {
            version: MOCK_VERSION.into(),
            dismissed: false,
            notes: MOCK_NOTES.into(),
        }
    } else {
        UpdatePhase::Idle
    };
    set_phase(app, phase, None);
}

/// Check GitHub for a newer release and offer it. `manual` (tray item or
/// settings button) reports the outcome in the phase and tray status line;
/// the silent startup check reports nothing but an available release.
async fn check(app: AppHandle, manual: bool) {
    if matches!(
        phase(&app),
        UpdatePhase::Checking | UpdatePhase::Installing { .. }
    ) {
        return;
    }
    // Dev builds must never self-update: a checkout older than the newest
    // GitHub release would install it over the debug binary.
    if cfg!(debug_assertions) {
        if manual {
            fail(&app, "Updates disabled in dev builds");
        }
        return;
    }
    set_phase(&app, UpdatePhase::Checking, None);
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            eprintln!("updater unavailable: {e}");
            return finish_check(&app, manual, "Update check unavailable");
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let notes = update.body.clone().unwrap_or_default();
            let phase = available(&app, update.version.clone(), notes);
            set_phase(&app, phase, Some(update));
        }
        Ok(None) => {
            if manual {
                set_status(&app, "Up to date");
                set_phase(&app, UpdatePhase::UpToDate, None);
            } else {
                set_phase(&app, UpdatePhase::Idle, None);
            }
        }
        Err(e) => {
            eprintln!("update check failed: {e}");
            finish_check(&app, manual, "Update check failed");
        }
    }
}

/// A check that could not complete: surfaced when manual, silent otherwise.
fn finish_check(app: &AppHandle, manual: bool, reason: &str) {
    if manual {
        fail(app, reason);
    } else {
        set_phase(app, UpdatePhase::Idle, None);
    }
}

/// Download and install the offered release, then relaunch.
async fn install(app: AppHandle) {
    let update = {
        let state = app.state::<UpdateState>();
        let mut inner = state.0.lock().unwrap();
        match inner.phase {
            UpdatePhase::Available { .. } => inner.pending.take(),
            _ => return,
        }
    };
    let Some(update) = update else {
        // Only the dev override offers a release without one behind it.
        return fail(&app, "Updates disabled in dev builds");
    };
    set_phase(
        &app,
        UpdatePhase::Installing {
            version: update.version.clone(),
        },
        None,
    );
    set_status(&app, "Downloading update…");
    // Progress goes out as a fraction whenever the whole percentage moves, so
    // the settings card can draw the download without a flood of events.
    let mut downloaded = 0u64;
    let mut last_percent = u64::MAX;
    let on_chunk = {
        let app = app.clone();
        move |chunk: usize, total: Option<u64>| {
            downloaded += chunk as u64;
            let Some(total) = total.filter(|t| *t > 0) else {
                return;
            };
            let percent = downloaded * 100 / total;
            if percent != last_percent {
                last_percent = percent;
                let _ = app.emit("update://progress", downloaded as f64 / total as f64);
            }
        }
    };
    let on_finish = {
        let app = app.clone();
        move || {
            let _ = app.emit("update://progress", 1.0);
        }
    };
    match update.download_and_install(on_chunk, on_finish).await {
        Ok(()) => {
            // Persist before the installer relaunches us; restart() never returns.
            crate::state::save(&app);
            app.restart();
        }
        Err(e) => {
            eprintln!("update install failed: {e}");
            fail(&app, "Update failed — try again later");
        }
    }
}

fn fail(app: &AppHandle, reason: &str) {
    set_status(app, reason);
    set_phase(
        app,
        UpdatePhase::Failed {
            reason: reason.into(),
        },
        None,
    );
}

/// Show a transient message in the tray status line. The next poll (≤60s)
/// restores the usage figures.
fn set_status(app: &AppHandle, msg: &str) {
    if let Some(handles) = app.try_state::<crate::tray::TrayHandles>() {
        let _ = handles.status_item.set_text(msg);
    }
}
