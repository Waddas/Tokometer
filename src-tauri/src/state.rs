use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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

    /// The layout's design-space dimensions (geometry in styles.css).
    fn design_size(self) -> (f64, f64) {
        match self {
            Layout::MascotLeft | Layout::MascotRight => (282.0, 168.0),
            Layout::MascotTop | Layout::MascotBottom => (238.0, 243.0),
            Layout::TilesRow => (238.0, 93.0),
            Layout::TilesColumn => (128.0, 168.0),
        }
    }

    /// Logical window size: the layout's design space scaled by the chosen
    /// `Size`, plus the 28px strip above the widget that hosts the hover
    /// controls. The frontend recomputes `--scale` from the resized width.
    pub fn window_size(self, size: Size) -> (f64, f64) {
        const CONTROLS_STRIP: f64 = 28.0;
        let (w, h) = self.design_size();
        let f = size.scale();
        (w * f, h * f + CONTROLS_STRIP)
    }
}

/// Overall widget scale. Small is the original 2/3 of the design space;
/// Medium and Large step around it. The window resize alone drives the
/// frontend's `--scale`, so the content fills whichever size is chosen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Size {
    #[default]
    Small,
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Medium, Size::Large];

    /// Stable id; matches the serde kebab-case serialization.
    pub fn id(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Medium => "medium",
            Size::Large => "large",
        }
    }

    pub fn from_id(id: &str) -> Option<Size> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Fraction of the design space the window occupies.
    pub fn scale(self) -> f64 {
        match self {
            Size::Small => 2.0 / 3.0,
            Size::Medium => 1.0,
            Size::Large => 4.0 / 3.0,
        }
    }
}

/// Which mascot the splash animates. Tiles-only layouts hide it regardless.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mascot {
    #[default]
    Clawd,
    Axolotl,
    Cat,
}

impl Mascot {
    pub const ALL: [Mascot; 3] = [Mascot::Clawd, Mascot::Axolotl, Mascot::Cat];

    /// Stable id; matches the serde serialization and the frontend's
    /// `Mascot` union / `MascotId` registry keys.
    pub fn id(self) -> &'static str {
        match self {
            Mascot::Clawd => "clawd",
            Mascot::Axolotl => "axolotl",
            Mascot::Cat => "cat",
        }
    }

    pub fn from_id(id: &str) -> Option<Mascot> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }
}

/// Which weekdays count as "work days", indexed Sun..Sat to match the
/// frontend's `Date.getDay()`. Unchecked days hold the 7-day prediction flat
/// (no usage expected), so the dotted line doesn't extrapolate across them.
pub fn all_work_days() -> [bool; 7] {
    [true; 7]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    /// Logical (DPI-independent) window position.
    pub window: Option<WindowPos>,
    pub pin: bool,
    pub layout: Layout,
    pub size: Size,
    pub mascot: Mascot,
    /// All-true by default; a plain derive would flatten the whole prediction.
    #[serde(default = "all_work_days")]
    pub work_days: [bool; 7],
    pub last_usage: Option<UsageSnapshot>,
}

// Hand-written so `work_days` defaults to all-true; `#[derive(Default)]` and
// serde's field default both leave it `[false; 7]`, flattening the prediction.
impl Default for PersistedState {
    fn default() -> Self {
        Self {
            window: None,
            pin: false,
            layout: Layout::default(),
            size: Size::default(),
            mascot: Mascot::default(),
            work_days: all_work_days(),
            last_usage: None,
        }
    }
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
    // Write-then-rename so a crash or a concurrent save (the poller thread and a
    // tray/UI action can both call this) can never leave a truncated state.json.
    // A corrupt file would make load() silently fall back to *all* defaults,
    // discarding the user's mascot, layout, pin and window position.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!("{}.tmp", SEQ.fetch_add(1, Ordering::Relaxed)));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
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
    fn every_layout_has_a_positive_window_size_at_every_size() {
        for layout in Layout::ALL {
            for size in Size::ALL {
                let (w, h) = layout.window_size(size);
                assert!(w > 0.0 && h > 0.0, "{:?}/{:?} has a non-positive size", layout, size);
            }
        }
    }

    #[test]
    fn larger_sizes_make_larger_windows() {
        for layout in Layout::ALL {
            let (sw, sh) = layout.window_size(Size::Small);
            let (mw, mh) = layout.window_size(Size::Medium);
            let (lw, lh) = layout.window_size(Size::Large);
            assert!(sw < mw && mw < lw, "{:?} width does not grow with size", layout);
            assert!(sh < mh && mh < lh, "{:?} height does not grow with size", layout);
        }
    }

    #[test]
    fn small_keeps_the_original_two_thirds_scale() {
        // The original window was the design space x 2/3; Small must match it
        // so existing users see no change after upgrading.
        let (w, h) = Layout::MascotLeft.window_size(Size::Small);
        assert_eq!((w, h), (188.0, 112.0 + 28.0));
    }

    #[test]
    fn size_id_round_trips_and_rejects_unknown() {
        for size in Size::ALL {
            assert_eq!(Size::from_id(size.id()), Some(size));
        }
        assert_eq!(Size::from_id("huge"), None);
    }

    #[test]
    fn default_size_is_small() {
        assert_eq!(Size::default(), Size::Small);
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
    fn mascot_id_round_trips_and_rejects_unknown() {
        for mascot in Mascot::ALL {
            assert_eq!(Mascot::from_id(mascot.id()), Some(mascot));
        }
        assert_eq!(Mascot::from_id("dragon"), None);
    }

    #[test]
    fn mascot_ids_match_the_frontend_union() {
        // Contract with src/api.ts `Mascot` and src/mascots.ts `MASCOTS` keys.
        assert_eq!(Mascot::Clawd.id(), "clawd");
        assert_eq!(Mascot::Axolotl.id(), "axolotl");
        assert_eq!(Mascot::Cat.id(), "cat");
    }

    #[test]
    fn default_mascot_is_clawd() {
        assert_eq!(Mascot::default(), Mascot::Clawd);
    }

    #[test]
    fn persisted_state_fills_missing_fields_with_defaults() {
        // The poller writes partial state early on; load() must tolerate it.
        let s: PersistedState = serde_json::from_str("{}").unwrap();
        assert!(!s.pin);
        assert_eq!(s.layout, Layout::MascotLeft);
        assert_eq!(s.size, Size::Small);
        assert_eq!(s.mascot, Mascot::Clawd);
        // All-true, not the [false; 7] a plain field default would give —
        // otherwise an old state.json (no workDays key) flattens the prediction.
        assert_eq!(s.work_days, [true; 7]);
        assert!(s.window.is_none());
        assert!(s.last_usage.is_none());
    }

    #[test]
    fn persisted_state_round_trips_through_json() {
        let original = PersistedState {
            window: Some(WindowPos { x: 12.0, y: 34.0 }),
            pin: true,
            layout: Layout::TilesRow,
            size: Size::Large,
            mascot: Mascot::Axolotl,
            work_days: [true, false, true, true, true, true, false],
            last_usage: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pin, original.pin);
        assert_eq!(back.layout, original.layout);
        assert_eq!(back.size, original.size);
        assert_eq!(back.mascot, original.mascot);
        assert_eq!(back.work_days, original.work_days);
        assert_eq!(back.window.unwrap().x, 12.0);
    }

    #[test]
    fn persisted_state_uses_camel_case_keys() {
        let s = PersistedState { pin: true, ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("lastUsage").is_some());
    }
}
