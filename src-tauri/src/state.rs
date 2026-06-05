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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trips_through_from_id_for_every_layout() {
        for layout in Layout::ALL {
            assert_eq!(Layout::from_id(layout.id()), Some(layout));
        }
    }

    #[test]
    fn from_id_rejects_unknown_ids() {
        assert_eq!(Layout::from_id("mascot-diagonal"), None);
        assert_eq!(Layout::from_id(""), None);
    }

    #[test]
    fn ids_match_the_frontend_union() {
        // These strings are the contract with src/api.ts `Layout` and the
        // `layout-*` body classes — changing one side must change the other.
        assert_eq!(Layout::MascotLeft.id(), "mascot-left");
        assert_eq!(Layout::MascotRight.id(), "mascot-right");
        assert_eq!(Layout::MascotTop.id(), "mascot-top");
        assert_eq!(Layout::MascotBottom.id(), "mascot-bottom");
        assert_eq!(Layout::TilesRow.id(), "tiles-row");
        assert_eq!(Layout::TilesColumn.id(), "tiles-column");
    }

    #[test]
    fn every_layout_has_a_positive_window_size() {
        for layout in Layout::ALL {
            let (w, h) = layout.window_size();
            assert!(w > 0.0 && h > 0.0, "{:?} has a non-positive size", layout);
        }
    }

    #[test]
    fn default_layout_is_mascot_left() {
        assert_eq!(Layout::default(), Layout::MascotLeft);
    }

    #[test]
    fn layout_serializes_as_kebab_case() {
        let v = serde_json::to_value(Layout::TilesColumn).unwrap();
        assert_eq!(v, serde_json::json!("tiles-column"));
    }

    #[test]
    fn persisted_state_fills_missing_fields_with_defaults() {
        // The poller writes partial state early on; load() must tolerate it.
        let s: PersistedState = serde_json::from_str("{}").unwrap();
        assert!(!s.pin);
        assert_eq!(s.layout, Layout::MascotLeft);
        assert!(s.window.is_none());
        assert!(s.last_usage.is_none());
    }

    #[test]
    fn persisted_state_round_trips_through_json() {
        let original = PersistedState {
            window: Some(WindowPos { x: 12.0, y: 34.0 }),
            pin: true,
            layout: Layout::TilesRow,
            last_usage: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pin, original.pin);
        assert_eq!(back.layout, original.layout);
        assert_eq!(back.window.unwrap().x, 12.0);
    }

    #[test]
    fn persisted_state_uses_camel_case_keys() {
        let s = PersistedState { pin: true, ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("lastUsage").is_some());
    }
}
