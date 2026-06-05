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

/// Where the widget places the mascot relative to the usage tiles,
/// or tiles only ("no mascot").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    #[default]
    MascotLeft,
    MascotRight,
    MascotTop,
    MascotBottom,
    TilesRow,
    TilesColumn,
}

impl Layout {
    pub const ALL: [Layout; 6] = [
        Layout::MascotLeft,
        Layout::MascotRight,
        Layout::MascotTop,
        Layout::MascotBottom,
        Layout::TilesRow,
        Layout::TilesColumn,
    ];

    /// Stable id; matches the serde kebab-case serialization and the
    /// frontend's `Layout` union / `layout-*` body classes.
    pub fn id(self) -> &'static str {
        match self {
            Layout::MascotLeft => "mascot-left",
            Layout::MascotRight => "mascot-right",
            Layout::MascotTop => "mascot-top",
            Layout::MascotBottom => "mascot-bottom",
            Layout::TilesRow => "tiles-row",
            Layout::TilesColumn => "tiles-column",
        }
    }

    pub fn from_id(id: &str) -> Option<Layout> {
        Self::ALL.into_iter().find(|l| l.id() == id)
    }

    /// Logical window size: the layout's design space scaled by 2/3
    /// (see the design-space dimensions in styles.css).
    pub fn window_size(self) -> (f64, f64) {
        match self {
            Layout::MascotLeft | Layout::MascotRight => (188.0, 112.0),
            Layout::MascotTop | Layout::MascotBottom => (159.0, 162.0),
            Layout::TilesRow => (159.0, 62.0),
            Layout::TilesColumn => (85.0, 112.0),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    /// Logical (DPI-independent) window position.
    pub window: Option<WindowPos>,
    pub pin: bool,
    pub layout: Layout,
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
