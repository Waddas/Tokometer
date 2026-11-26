//! Usage status for coding sessions: a small, versioned JSON file written next
//! to state.json after every published poll. The Claude Code plugin in
//! `plugin/` reads it, so its field names and pretty-printed layout (one
//! top-level scalar per line) are a contract with
//! `plugin/scripts/tokometer-usage.sh`.

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::history::Sample;
use crate::usage::{LimitWindow, UsageSnapshot, ID_SESSION};

pub const VERSION: u32 = 1;
const FILE_NAME: &str = "usage-status.json";

/// Any window at or above this is critical.
const CRITICAL_PCT: f64 = 95.0;
/// Any window at or above this is a warning.
const WARN_PCT: f64 = 80.0;
/// Below this, a forecast run-out is too early in the window to trust.
const FORECAST_MIN_PCT: f64 = 50.0;
/// Samples whose recorded reset differs by more than this belong to another
/// window (matches RESET_TOLERANCE_MS in src/history.ts).
const RESET_TOLERANCE_MS: i64 = 90_000;
const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// Ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// No usable windows (provider error with nothing retained).
    Unknown,
    Ok,
    Warn,
    Critical,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStatus {
    pub id: String,
    pub label: String,
    /// 0–100 percent.
    pub utilization: f64,
    /// Unix epoch seconds.
    pub resets_at: Option<i64>,
    /// A last known value carried over from an earlier poll.
    pub stale: bool,
    /// Unix epoch seconds when the current pace reaches 100%, if before reset
    /// and the window is at least FORECAST_MIN_PCT used.
    pub runs_out_at: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusFile {
    pub version: u32,
    pub provider: &'static str,
    /// "ok" | "error", from the latest poll.
    pub status: String,
    /// Unix epoch seconds of the latest successful reading the windows reflect.
    pub observed_at: i64,
    pub level: Level,
    /// One line describing every window, for showing to a session as-is.
    pub summary: String,
    pub windows: Vec<WindowStatus>,
}

/// Trend span per window, matching windowSpan in src/graph.ts.
fn trend_span_ms(id: &str, window_seconds: Option<i64>) -> i64 {
    if let Some(secs) = window_seconds.filter(|&s| s > 0) {
        let window_ms = secs * 1000;
        let trend_ms = if window_ms > DAY_MS {
            6 * HOUR_MS
        } else {
            30 * MINUTE_MS
        };
        return trend_ms.min(window_ms);
    }
    if id == ID_SESSION {
        30 * MINUTE_MS
    } else if id.starts_with("monthly") {
        DAY_MS
    } else {
        6 * HOUR_MS
    }
}

/// Least-squares usage rate (percent per ms) over points within `span_ms` of
/// `now`, or None until they cover an eighth of the span. Port of trendSlope
/// in src/trend.ts.
fn trend_slope(pts: &[(i64, f64)], span_ms: i64, now: i64) -> Option<f64> {
    let recent: Vec<_> = pts.iter().filter(|(ms, _)| now - ms <= span_ms).collect();
    let (first, last) = (recent.first()?, recent.last()?);
    if recent.len() < 2 || last.0 - first.0 < span_ms / 8 {
        return None;
    }
    let (mut st, mut sp, mut stt, mut stp) = (0.0, 0.0, 0.0, 0.0);
    for (ms, pct) in &recent {
        let t = (ms - first.0) as f64;
        st += t;
        sp += pct;
        stt += t * t;
        stp += t * pct;
    }
    let n = recent.len() as f64;
    let denom = n * stt - st * st;
    (denom != 0.0).then(|| (n * stp - st * sp) / denom)
}

/// When the window's current pace reaches 100%, in epoch ms, if before reset.
/// Unlike the graph, it ramps through days left out of the work-day setting.
fn runs_out_at_ms(
    samples: &[Sample],
    scope: &str,
    w: &LimitWindow,
    reset_ms: i64,
    now: i64,
) -> Option<i64> {
    let (id, utilization) = (w.id.as_str(), w.utilization);
    if !(FORECAST_MIN_PCT..100.0).contains(&utilization) {
        return None;
    }
    let mut pts: Vec<(i64, f64)> = samples
        .iter()
        .filter(|s| s.scope == scope && s.ms <= now)
        .filter_map(|s| {
            let w = s.w.get(id)?;
            let same_window = w
                .reset
                .is_none_or(|r| (r - reset_ms).abs() <= RESET_TOLERANCE_MS);
            same_window.then_some((s.ms, w.pct))
        })
        .collect();
    pts.push((now, utilization));
    let slope = trend_slope(&pts, trend_span_ms(id, w.window_seconds), now)?;
    if slope <= 0.0 {
        return None;
    }
    // In f64: a near-zero slope from rounding would overflow an i64.
    let at = now as f64 + (100.0 - utilization) / slope;
    (at < reset_ms as f64).then_some(at as i64)
}

fn level_of(w: &WindowStatus) -> Level {
    if w.utilization >= CRITICAL_PCT {
        Level::Critical
    } else if w.utilization >= WARN_PCT || w.runs_out_at.is_some() {
        Level::Warn
    } else {
        Level::Ok
    }
}

/// "45m", "1h 12m", "3d 4h".
fn duration(ms: i64) -> String {
    let mins = (ms.max(0) + MINUTE_MS - 1) / MINUTE_MS;
    let (d, h, m) = (mins / 1440, mins % 1440 / 60, mins % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn summary(provider: &str, windows: &[WindowStatus], now: i64) -> String {
    let parts: Vec<String> = windows
        .iter()
        .map(|w| {
            let mut s = format!("{} {:.0}%", w.label, w.utilization);
            if let Some(r) = w.resets_at {
                s += &format!(" (resets in {})", duration(r * 1000 - now));
            }
            if w.stale {
                s += " [last known]";
            }
            if let Some(out) = w.runs_out_at {
                s += &format!(
                    " [on pace to hit 100% in {}, before reset]",
                    duration(out * 1000 - now)
                );
            }
            s
        })
        .collect();
    format!("{provider} usage: {}.", parts.join(", "))
}

pub fn build(snapshot: &UsageSnapshot, samples: &[Sample], now: i64) -> StatusFile {
    let windows: Vec<WindowStatus> = snapshot
        .windows
        .iter()
        // A carried-over window whose reset has passed no longer holds its value.
        .filter(|w| w.reset_at.is_none_or(|r| r * 1000 > now))
        .map(|w| WindowStatus {
            id: w.id.clone(),
            label: w.label.clone(),
            utilization: w.utilization,
            resets_at: w.reset_at,
            stale: w.stale,
            runs_out_at: w.reset_at.and_then(|r| {
                runs_out_at_ms(samples, &snapshot.scope, w, r * 1000, now).map(|ms| ms / 1000)
            }),
        })
        .collect();
    let level = windows.iter().map(level_of).max().unwrap_or(Level::Unknown);
    let provider = snapshot.provider.label();
    let summary = if windows.is_empty() {
        let reason = snapshot.error.as_deref().unwrap_or("no limits reported");
        format!("{provider} usage unavailable: {reason}")
    } else {
        summary(provider, &windows, now)
    };
    let observed_ms = if snapshot.status == "ok" {
        snapshot.fetched_at
    } else {
        snapshot.last_success_at.unwrap_or(snapshot.fetched_at)
    };
    StatusFile {
        version: VERSION,
        provider: snapshot.provider.id(),
        status: snapshot.status.clone(),
        observed_at: observed_ms / 1000,
        level,
        summary,
        windows,
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(FILE_NAME))
}

pub fn write(app: &AppHandle, status: &StatusFile) {
    let Some(path) = path(app) else { return };
    if let Ok(json) = serde_json::to_string_pretty(status) {
        crate::state::write_atomic(&path, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::WindowSample;
    use std::collections::BTreeMap;

    const NOW: i64 = 1_800_000_000_000;

    fn window(id: &str, label: &str, pct: f64, reset_in_ms: i64) -> LimitWindow {
        LimitWindow {
            id: id.into(),
            label: label.into(),
            utilization: pct,
            reset_at: Some((NOW + reset_in_ms) / 1000),
            stale: false,
            window_seconds: None,
        }
    }

    fn sample(ms: i64, id: &str, pct: f64, reset_ms: i64) -> Sample {
        let mut w = BTreeMap::new();
        w.insert(
            id.into(),
            WindowSample {
                pct,
                reset: Some(reset_ms),
            },
        );
        Sample {
            scope: "claude".into(),
            ms,
            w,
        }
    }

    fn snapshot(windows: Vec<LimitWindow>) -> UsageSnapshot {
        let mut s = UsageSnapshot::ok("oauth", windows);
        s.fetched_at = NOW;
        s
    }

    #[test]
    fn levels_follow_the_worst_window() {
        let cases = [
            (10.0, Level::Ok),
            (80.0, Level::Warn),
            (95.0, Level::Critical),
        ];
        for (pct, expected) in cases {
            let snap = snapshot(vec![
                window("session", "5h", 5.0, HOUR_MS),
                window("weekly_all", "7d", pct, 3 * DAY_MS),
            ]);
            let status = build(&snap, &[], NOW);
            assert_eq!(status.level, expected, "{pct}%");
        }
    }

    #[test]
    fn fast_pace_warns_before_the_threshold() {
        let reset = NOW + 2 * HOUR_MS;
        // 40% → 60% over the last 20 minutes: 1%/min, so 100% in 40 minutes.
        let samples: Vec<_> = (0..=4)
            .map(|i| {
                sample(
                    NOW - (20 - i * 5) * MINUTE_MS,
                    "session",
                    40.0 + i as f64 * 5.0,
                    reset,
                )
            })
            .collect();
        let snap = snapshot(vec![window("session", "5h", 60.0, 2 * HOUR_MS)]);
        let status = build(&snap, &samples, NOW);
        assert_eq!(status.level, Level::Warn);
        let out = status.windows[0].runs_out_at.unwrap();
        assert!((out - (NOW + 40 * MINUTE_MS) / 1000).abs() <= 60, "{out}");
        assert!(
            status.summary.contains("on pace to hit 100% in 40m"),
            "{}",
            status.summary
        );
    }

    #[test]
    fn pace_ignores_samples_from_a_previous_window() {
        let reset = NOW + 4 * HOUR_MS;
        let old_reset = NOW - HOUR_MS;
        // A steep climb in the previous window must not carry into this one.
        let samples = vec![
            sample(NOW - 25 * MINUTE_MS, "session", 70.0, old_reset),
            sample(NOW - 20 * MINUTE_MS, "session", 90.0, old_reset),
            sample(NOW - 10 * MINUTE_MS, "session", 52.0, reset),
        ];
        let snap = snapshot(vec![window("session", "5h", 52.0, 4 * HOUR_MS)]);
        let status = build(&snap, &samples, NOW);
        assert_eq!(status.windows[0].runs_out_at, None);
        assert_eq!(status.level, Level::Ok);
    }

    #[test]
    fn flat_usage_with_rounding_noise_has_no_forecast() {
        let reset = NOW + 4 * HOUR_MS;
        // Real history holds values like 55.00000000000001. This flat run of
        // jittered polls fits a slope of ~4e-20 %/ms, which used to overflow.
        let pct = 55.00000000000001;
        let samples: Vec<_> = (0..13)
            .map(|i| {
                sample(
                    NOW - 1500 - (12 - i) * MINUTE_MS - i * 37 % 900,
                    "session",
                    pct,
                    reset,
                )
            })
            .collect();
        let snap = snapshot(vec![window("session", "5h", pct, 4 * HOUR_MS)]);
        let status = build(&snap, &samples, NOW);
        assert_eq!(status.windows[0].runs_out_at, None);
        assert_eq!(status.level, Level::Ok);
    }

    #[test]
    fn early_forecast_below_half_is_left_out() {
        let reset = NOW + 4 * HOUR_MS;
        let samples = vec![
            sample(NOW - 20 * MINUTE_MS, "session", 10.0, reset),
            sample(NOW - 10 * MINUTE_MS, "session", 20.0, reset),
        ];
        let snap = snapshot(vec![window("session", "5h", 30.0, 4 * HOUR_MS)]);
        let status = build(&snap, &samples, NOW);
        assert_eq!(status.windows[0].runs_out_at, None);
        assert_eq!(status.level, Level::Ok);
        assert!(!status.summary.contains("on pace"), "{}", status.summary);
    }

    #[test]
    fn error_without_windows_is_unknown_and_keeps_last_success_time() {
        let mut snap = UsageSnapshot::ok("oauth", vec![]);
        snap.status = "error".into();
        snap.error = Some("token expired".into());
        snap.fetched_at = NOW;
        snap.last_success_at = Some(NOW - HOUR_MS);
        let status = build(&snap, &[], NOW);
        assert_eq!(status.level, Level::Unknown);
        assert_eq!(status.observed_at, (NOW - HOUR_MS) / 1000);
        assert!(status.summary.contains("token expired"));
    }

    #[test]
    fn summary_lists_every_window_with_reset_times() {
        let snap = snapshot(vec![
            window("session", "5h", 87.0, HOUR_MS + 12 * MINUTE_MS),
            window("weekly_all", "7d", 61.0, 3 * DAY_MS + 4 * HOUR_MS),
        ]);
        let status = build(&snap, &[], NOW);
        assert_eq!(
            status.summary,
            "Claude usage: 5h 87% (resets in 1h 12m), 7d 61% (resets in 3d 4h)."
        );
    }

    #[test]
    fn windows_past_their_reset_are_left_out() {
        let mut expired = window("weekly_scoped:fable", "Fable", 99.0, -DAY_MS);
        expired.stale = true;
        let snap = snapshot(vec![window("session", "5h", 20.0, HOUR_MS), expired]);
        let status = build(&snap, &[], NOW);
        assert_eq!(status.windows.len(), 1);
        assert_eq!(status.level, Level::Ok);
        assert!(!status.summary.contains("Fable"), "{}", status.summary);
    }

    /// The plugin's shell script reads top-level scalars line by line.
    #[test]
    fn pretty_json_keeps_scalars_on_their_own_lines() {
        let snap = snapshot(vec![window("session", "5h", 81.0, HOUR_MS)]);
        let json = serde_json::to_string_pretty(&build(&snap, &[], NOW)).unwrap();
        for line in [
            "  \"version\": 1,",
            "  \"provider\": \"claude\",",
            "  \"level\": \"warn\",",
        ] {
            assert!(json.lines().any(|l| l == line), "missing {line}\n{json}");
        }
        assert!(json.lines().any(|l| l.starts_with("  \"observedAt\": ")));
        assert!(json.lines().any(|l| l.starts_with("  \"summary\": \"")));
    }
}
