use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::usage::UsageSnapshot;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Claude,
    Codex,
}
impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }
}

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

    /// The layout's design-space dimensions (geometry in styles.css). Fixed:
    /// however many limits the API reports, the tiles share the layout's tile
    /// band, so the window never resizes itself around the data.
    pub(crate) fn design_size(self) -> (f64, f64) {
        match self {
            Layout::MascotLeft | Layout::MascotRight => (282.0, 168.0),
            Layout::MascotTop | Layout::MascotBottom => (238.0, 243.0),
            Layout::TilesRow => (238.0, 93.0),
            Layout::TilesColumn => (128.0, 168.0),
        }
    }

    /// Default toolbar placement, for the layout geometry regression checks.
    #[cfg(test)]
    pub fn window_size(self, scale: f64) -> (f64, f64) {
        let g = crate::chrome::Geometry::new(
            self,
            scale,
            crate::chrome::Side::Top,
            crate::chrome::Side::Right,
        );
        (g.width, g.height)
    }

    /// The free-resize scale a window of logical width `width` implies.
    pub fn scale_for_width(self, width: f64) -> f64 {
        let (design_w, _) = self.design_size();
        (width / design_w).clamp(MIN_SCALE, MAX_SCALE)
    }
}

/// Overall widget scale. Small is the original 2/3 of the design space;
/// Medium and Large step around it. The window resize alone drives the
/// frontend's layout, so the content fills whichever size is chosen.
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

/// Bounds for the free-resize scale — small enough to tuck away, large
/// enough for a 4K/high-DPI display, and safely inside every preset.
pub const MIN_SCALE: f64 = 0.5;
pub const MAX_SCALE: f64 = 2.5;

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

/// How the tray icon renders the 5h figure: a colour-coded progress ring, or
/// the figure as text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrayStyle {
    #[default]
    Ring,
    Text,
}

impl TrayStyle {
    pub const ALL: [TrayStyle; 2] = [TrayStyle::Ring, TrayStyle::Text];

    /// Stable id; matches the serde kebab-case serialization.
    pub fn id(self) -> &'static str {
        match self {
            TrayStyle::Ring => "ring",
            TrayStyle::Text => "text",
        }
    }

    pub fn from_id(id: &str) -> Option<TrayStyle> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Shared appearance for the widget and settings window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Charcoal,
    Midnight,
    Paper,
    Mist,
}

impl Theme {
    pub const ALL: [Theme; 4] = [Theme::Charcoal, Theme::Midnight, Theme::Paper, Theme::Mist];

    pub fn id(self) -> &'static str {
        match self {
            Theme::Charcoal => "charcoal",
            Theme::Midnight => "midnight",
            Theme::Paper => "paper",
            Theme::Mist => "mist",
        }
    }

    pub fn from_id(id: &str) -> Option<Theme> {
        Self::ALL.into_iter().find(|theme| theme.id() == id)
    }

    pub fn window_theme(self) -> tauri::Theme {
        match self {
            Theme::Charcoal | Theme::Midnight => tauri::Theme::Dark,
            Theme::Paper | Theme::Mist => tauri::Theme::Light,
        }
    }

    /// Native background before the settings webview paints; matches --bg in theme.css.
    pub fn background_color(self) -> tauri::window::Color {
        let (r, g, b) = match self {
            Theme::Charcoal => (0x14, 0x15, 0x16),
            Theme::Midnight => (0x05, 0x05, 0x05),
            Theme::Paper => (0xf0, 0xec, 0xe4),
            Theme::Mist => (0xe9, 0xef, 0xf4),
        };
        tauri::window::Color(r, g, b, 255)
    }
}

/// Which weekdays count as "work days", indexed Sun..Sat to match the
/// frontend's `Date.getDay()`. Unchecked days hold the 7-day prediction flat
/// (no usage expected), so the dotted line doesn't extrapolate across them.
pub fn all_work_days() -> [bool; 7] {
    [true; 7]
}

/// Opt-in previews of unfinished features. Every flag defaults to off, and
/// turning one off restores the previous behaviour. None are being trialled
/// right now; to add one, declare a `bool` field here, mirror it in
/// `BetaFeatures` (api.ts) and give it a checkbox in the settings window
/// (`BETA_BOXES` in settings.ts). A flag retired from here is ignored when an
/// older state.json still carries it, so removing a feature needs no migration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BetaFeatures {}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ProviderAvailability {
    pub claude: bool,
    pub codex: bool,
}
impl ProviderAvailability {
    pub fn both(self) -> bool { self.claude && self.codex }
    pub fn allows(self, provider: Provider) -> bool {
        match provider { Provider::Claude => self.claude, Provider::Codex => self.codex }
    }
    pub fn update(&mut self, provider: Provider, available: bool) -> bool {
        let value = match provider { Provider::Claude => &mut self.claude, Provider::Codex => &mut self.codex };
        let changed = *value != available;
        *value = available;
        changed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersistedState {
    pub controls_side: crate::chrome::Side,
    pub providers_side: crate::chrome::Side,
    pub provider: Provider,
    pub provider_hidden_limits: std::collections::BTreeMap<String, Vec<String>>,
    /// Invalidates polls started before a provider switch, including A → B → A.
    #[serde(skip)]
    pub provider_revision: u64,
    /// Local provider presence, rechecked each launch and refresh.
    #[serde(skip)]
    pub available_providers: ProviderAvailability,
    /// Logical (DPI-independent) window position.
    pub window: Option<WindowPos>,
    pub pin: bool,
    pub layout: Layout,
    pub size: Size,
    /// Free-resize scale; overrides `size` while set, cleared by picking a preset.
    pub custom_scale: Option<f64>,
    pub mascot: Mascot,
    pub tray_style: TrayStyle,
    pub theme: Theme,
    /// All-true by default; a plain derive would flatten the whole prediction.
    #[serde(default = "all_work_days")]
    pub work_days: [bool; 7],
    /// Whether a failing usage endpoint may fall back to a minimal (1-token,
    /// quota-consuming) `/v1/messages` probe. On by default.
    pub probe_fallback: bool,
    /// Ids of limit windows the user hid; ids the API no longer reports are
    /// kept, so a limit that comes back stays hidden.
    #[serde(default)]
    pub hidden_limits: Vec<String>,
    #[serde(default)]
    pub beta: BetaFeatures,
    /// Release whose update-available dots the user hid; a newer release
    /// shows them again.
    #[serde(default)]
    pub dismissed_update: Option<String>,
    pub last_usage: Option<UsageSnapshot>,
}

impl PersistedState {
    pub fn geometry(&self) -> crate::chrome::Geometry {
        crate::chrome::Geometry::with_providers(
            self.layout,
            self.effective_scale(),
            self.controls_side,
            self.providers_side,
            self.available_providers.both(),
        )
    }
    pub fn switch_provider(&mut self, provider: Provider) -> bool {
        if self.provider == provider {
            return false;
        }
        self.provider_hidden_limits
            .insert(self.provider.id().into(), self.hidden_limits.clone());
        self.hidden_limits = self
            .provider_hidden_limits
            .get(provider.id())
            .cloned()
            .unwrap_or_default();
        self.provider = provider;
        self.provider_revision += 1;
        self.last_usage = None;
        true
    }

    pub fn accepts_poll(&self, provider: Provider, revision: u64) -> bool {
        self.provider == provider && self.provider_revision == revision
    }

    /// The scale the window actually renders at.
    pub fn effective_scale(&self) -> f64 {
        self.custom_scale
            .unwrap_or(self.size.scale())
            .clamp(MIN_SCALE, MAX_SCALE)
    }
}

// Hand-written so `work_days` defaults to all-true; `#[derive(Default)]` and
// serde's field default both leave it `[false; 7]`, flattening the prediction.
impl Default for PersistedState {
    fn default() -> Self {
        Self {
            controls_side: crate::chrome::Side::Top,
            providers_side: crate::chrome::provider_side(),
            provider: Provider::default(),
            provider_hidden_limits: Default::default(),
            provider_revision: 0,
            available_providers: ProviderAvailability::default(),
            window: None,
            pin: false,
            layout: Layout::default(),
            size: Size::default(),
            custom_scale: None,
            mascot: Mascot::default(),
            tray_style: TrayStyle::default(),
            theme: Theme::default(),
            work_days: all_work_days(),
            probe_fallback: true,
            hidden_limits: Vec::new(),
            beta: BetaFeatures::default(),
            dismissed_update: None,
            last_usage: None,
        }
    }
}

pub struct AppState(pub Mutex<PersistedState>);

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("state.json"))
}

pub fn load(app: &AppHandle) -> PersistedState {
    state_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle) {
    let Some(path) = state_path(app) else { return };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let json = {
        let s = state.0.lock().unwrap();
        serde_json::to_string_pretty(&*s).unwrap()
    };
    write_atomic(&path, &json);
}

/// Write-then-rename so a crash or a concurrent save (the poller thread and a
/// tray/UI action can both call this) can never leave a truncated file — a
/// corrupt state.json would make load() silently fall back to *all* defaults,
/// discarding the user's layout, pin and window position.
pub fn write_atomic(path: &Path, contents: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!("{}.tmp", SEQ.fetch_add(1, Ordering::Relaxed)));
    if std::fs::write(&tmp, contents).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_requires_local_detection_for_each_provider_and_is_not_persisted() {
        let mut state = PersistedState::default();
        assert!(!state.available_providers.allows(Provider::Claude));
        assert!(!state.available_providers.both());
        assert!(state.available_providers.update(Provider::Codex, true));
        assert!(state.available_providers.allows(Provider::Codex));
        assert!(!state.available_providers.both());
        state.available_providers.update(Provider::Claude, true);
        assert!(state.available_providers.both());
        let saved = serde_json::to_string(&state).unwrap();
        let restored: PersistedState = serde_json::from_str(&saved).unwrap();
        assert!(!restored.available_providers.both());
        state.available_providers.update(Provider::Claude, false);
        assert!(!state.available_providers.both());
    }

    #[test]
    fn switching_provider_invalidates_in_flight_polls_and_restores_hidden_limits() {
        let mut state: PersistedState = serde_json::from_str(
            r#"{"hiddenLimits":["weekly_all"],"lastUsage":{"status":"ok","source":"oauth","fetchedAt":1,"windows":[],"error":null}}"#,
        ).unwrap();
        assert_eq!(state.provider, Provider::Claude);
        assert_eq!(state.last_usage.as_ref().unwrap().scope, "claude");
        let original_revision = state.provider_revision;
        assert!(state.switch_provider(Provider::Codex));
        assert!(state.last_usage.is_none());
        assert!(state.hidden_limits.is_empty());
        assert!(!state.accepts_poll(Provider::Claude, original_revision));
        state.hidden_limits = vec!["session".into()];
        assert!(state.switch_provider(Provider::Claude));
        assert_eq!(state.hidden_limits, ["weekly_all"]);
        // Switching back cannot make the original Claude request valid again.
        assert!(!state.accepts_poll(Provider::Claude, original_revision));
        assert!(!state.switch_provider(Provider::Claude));
        let json = serde_json::to_string(&state).unwrap();
        let mut restored: PersistedState = serde_json::from_str(&json).unwrap();
        restored.switch_provider(Provider::Codex);
        assert_eq!(restored.hidden_limits, ["session"]);
    }

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
                let (w, h) = layout.window_size(size.scale());
                assert!(
                    w > 0.0 && h > 0.0,
                    "{:?}/{:?} has a non-positive size",
                    layout,
                    size
                );
            }
        }
    }

    #[test]
    fn design_sizes_do_not_depend_on_the_data() {
        // The tiles share their band, so a new or hidden limit must never
        // resize the widget; these constants are the whole geometry contract
        // with styles.css (and `DESIGN_WIDTH` in main.ts).
        assert_eq!(Layout::MascotLeft.design_size(), (282.0, 168.0));
        assert_eq!(Layout::MascotRight.design_size(), (282.0, 168.0));
        assert_eq!(Layout::MascotTop.design_size(), (238.0, 243.0));
        assert_eq!(Layout::MascotBottom.design_size(), (238.0, 243.0));
        assert_eq!(Layout::TilesRow.design_size(), (238.0, 93.0));
        assert_eq!(Layout::TilesColumn.design_size(), (128.0, 168.0));
    }

    #[test]
    fn larger_sizes_make_larger_windows() {
        for layout in Layout::ALL {
            let (sw, sh) = layout.window_size(Size::Small.scale());
            let (mw, mh) = layout.window_size(Size::Medium.scale());
            let (lw, lh) = layout.window_size(Size::Large.scale());
            assert!(
                sw < mw && mw < lw,
                "{:?} width does not grow with size",
                layout
            );
            assert!(
                sh < mh && mh < lh,
                "{:?} height does not grow with size",
                layout
            );
        }
    }

    #[test]
    fn small_keeps_the_original_two_thirds_scale() {
        // The original window was the design space x 2/3; Small must match it
        // so existing users see no change after upgrading.
        let (w, h) = Layout::MascotLeft.window_size(Size::Small.scale());
        assert_eq!((w, h), (188.0 + 32.0, 112.0 + 32.0));
    }

    #[test]
    fn scale_for_width_inverts_window_size_within_bounds() {
        for layout in Layout::ALL {
            let (w, _) = layout.window_size(1.2);
            assert!((layout.scale_for_width(w - 32.0) - 1.2).abs() < 1e-9);
        }
        // Out-of-range widths clamp instead of producing absurd windows.
        assert_eq!(Layout::TilesRow.scale_for_width(10.0), MIN_SCALE);
        assert_eq!(Layout::TilesRow.scale_for_width(100_000.0), MAX_SCALE);
    }

    #[test]
    fn effective_scale_prefers_the_custom_scale_and_clamps_it() {
        let mut s = PersistedState::default();
        assert_eq!(s.effective_scale(), Size::Small.scale());
        s.custom_scale = Some(1.7);
        assert_eq!(s.effective_scale(), 1.7);
        s.custom_scale = Some(99.0);
        assert_eq!(s.effective_scale(), MAX_SCALE);
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
    fn tray_style_id_round_trips_and_rejects_unknown() {
        for style in TrayStyle::ALL {
            assert_eq!(TrayStyle::from_id(style.id()), Some(style));
        }
        assert_eq!(TrayStyle::from_id("bars"), None);
    }

    #[test]
    fn default_tray_style_is_ring() {
        assert_eq!(TrayStyle::default(), TrayStyle::Ring);
    }

    #[test]
    fn every_theme_survives_a_state_save_and_reload() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_id(theme.id()), Some(theme));
            let state = PersistedState {
                theme,
                pin: true,
                layout: Layout::TilesRow,
                ..Default::default()
            };
            let json = serde_json::to_string(&state).unwrap();
            let restored: PersistedState = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.theme, theme);
            assert!(restored.pin);
            assert_eq!(restored.layout, Layout::TilesRow);
        }
        assert_eq!(Theme::from_id("unknown"), None);
    }

    #[test]
    fn upgrading_an_older_state_defaults_theme_without_resetting_preferences() {
        let state: PersistedState = serde_json::from_str(
            r#"{"pin":true,"layout":"tiles-column","size":"large","hiddenLimits":["weekly_all"]}"#,
        )
        .unwrap();
        assert_eq!(state.theme, Theme::Charcoal);
        assert!(state.pin);
        assert_eq!(state.layout, Layout::TilesColumn);
        assert_eq!(state.size, Size::Large);
        assert_eq!(state.hidden_limits, vec!["weekly_all"]);
    }

    #[test]
    fn persisted_state_fills_missing_fields_with_defaults() {
        // The poller writes partial state early on; load() must tolerate it.
        let s: PersistedState = serde_json::from_str("{}").unwrap();
        assert!(!s.pin);
        assert_eq!(s.layout, Layout::MascotLeft);
        assert_eq!(s.size, Size::Small);
        assert_eq!(s.mascot, Mascot::Clawd);
        assert_eq!(s.tray_style, TrayStyle::Ring);
        assert_eq!(s.theme, Theme::Charcoal);
        // All-true, not the [false; 7] a plain field default would give —
        // otherwise an old state.json (no workDays key) flattens the prediction.
        assert_eq!(s.work_days, [true; 7]);
        assert!(s.window.is_none());
        assert!(s.custom_scale.is_none());
        // On by default so the app keeps working when the usage endpoint
        // rate-limits; the probe is cheap (1 token) and can be turned off.
        assert!(s.probe_fallback);
        // New limits are visible until the user hides them.
        assert!(s.hidden_limits.is_empty());
        assert_eq!(s.beta, BetaFeatures::default());
        assert!(s.last_usage.is_none());
    }

    #[test]
    fn persisted_state_ignores_retired_beta_flags() {
        // A state.json written while a since-removed beta feature was on.
        let s: PersistedState =
            serde_json::from_str(r#"{"beta":{"learnedForecast":true}}"#).unwrap();
        assert_eq!(s.beta, BetaFeatures::default());
    }

    #[test]
    fn persisted_state_round_trips_through_json() {
        let original = PersistedState {
            controls_side: crate::chrome::Side::Top,
            providers_side: crate::chrome::provider_side(),
            provider: Provider::default(),
            provider_hidden_limits: Default::default(),
            provider_revision: 0,
            available_providers: ProviderAvailability::default(),
            window: Some(WindowPos { x: 12.0, y: 34.0 }),
            pin: true,
            layout: Layout::TilesRow,
            size: Size::Large,
            custom_scale: Some(1.1),
            mascot: Mascot::Axolotl,
            tray_style: TrayStyle::Text,
            theme: Theme::Paper,
            work_days: [true, false, true, true, true, true, false],
            probe_fallback: true,
            hidden_limits: vec!["weekly_scoped:fable".into()],
            beta: BetaFeatures::default(),
            dismissed_update: Some("1.4.0".into()),
            last_usage: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hidden_limits, original.hidden_limits);
        assert_eq!(back.beta, original.beta);
        assert_eq!(back.dismissed_update, original.dismissed_update);
        assert_eq!(back.pin, original.pin);
        assert_eq!(back.layout, original.layout);
        assert_eq!(back.size, original.size);
        assert_eq!(back.custom_scale, original.custom_scale);
        assert_eq!(back.mascot, original.mascot);
        assert_eq!(back.tray_style, original.tray_style);
        assert_eq!(back.theme, original.theme);
        assert_eq!(back.work_days, original.work_days);
        assert_eq!(back.probe_fallback, original.probe_fallback);
        assert_eq!(back.window.unwrap().x, 12.0);
    }

    #[test]
    fn persisted_state_uses_camel_case_keys() {
        let s = PersistedState {
            pin: true,
            ..Default::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("lastUsage").is_some());
        assert!(v.get("hiddenLimits").is_some());
        assert!(v.get("beta").is_some());
    }
}
