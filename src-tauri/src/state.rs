use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::usage::UsageSnapshot;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowPos {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    /// Logical (DPI-independent) window position.
    pub window: Option<WindowPos>,
    pub pin: bool,
    pub last_usage: Option<UsageSnapshot>,
}

pub struct AppState(pub Mutex<PersistedState>);

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("state.json"))
}

pub fn load(app: &AppHandle) -> PersistedState {
    state_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle) {
    let Some(path) = state_path(app) else { return };
    let Some(state) = app.try_state::<AppState>() else { return };
    let json = {
        let s = state.0.lock().unwrap();
        serde_json::to_string_pretty(&*s).unwrap()
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, json);
}
