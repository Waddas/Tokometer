//! Backend usage-history log behind the graph. Lives next to state.json so it
//! survives WebView data clears, reinstalls, and the dev/prod origin split
//! that used to strand the frontend's localStorage copy.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::usage::{UsageSnapshot, ID_SESSION, ID_WEEKLY_ALL};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSample {
    /// 0-100 percent.
    pub pct: f64,
    /// The window's reset time (epoch ms), when the poll carried one.
    pub reset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// unix epoch ms
    pub ms: i64,
    /// window id → sample; absent ids mean the poll lacked that window
    #[serde(default)]
    pub w: BTreeMap<String, WindowSample>,
}

/// The on-disk sample shape: the keyed map plus the fixed 5h/7d fields builds
/// before dynamic limits wrote. Deserializing through this keeps old logs (and
/// ancient localStorage payloads) readable; only the map form is ever written.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSample {
    pub ms: i64,
    #[serde(default)]
    w: BTreeMap<String, WindowSample>,
    #[serde(default)]
    five: Option<f64>,
    #[serde(default)]
    week: Option<f64>,
    #[serde(default)]
    five_reset: Option<i64>,
    #[serde(default)]
    week_reset: Option<i64>,
}

impl RawSample {
    /// Whether this sample carries only the old fixed fields, so `normalize`
    /// has to convert it.
    fn is_legacy(&self) -> bool {
        self.w.is_empty() && (self.five.is_some() || self.week.is_some())
    }

    pub fn normalize(self) -> Sample {
        let mut w = self.w;
        if w.is_empty() {
            if let Some(pct) = self.five {
                let reset = self.five_reset;
                w.insert(ID_SESSION.into(), WindowSample { pct, reset });
            }
            if let Some(pct) = self.week {
                let reset = self.week_reset;
                w.insert(ID_WEEKLY_ALL.into(), WindowSample { pct, reset });
            }
        }
        Sample { ms: self.ms, w }
    }
}

// Keep several completed 7-day windows so the frontend can learn the user's
// usual ramp. At the sparse 5-minute cadence this is still a small JSON file.
const MAX_AGE_MS: i64 = 35 * 86_400_000;
const DENSE_AGE_MS: i64 = 6 * 3_600_000; // keep every sample this recent...
const SPARSE_GAP_MS: i64 = 5 * 60_000; // ...thin older ones to one per 5 min
const MIN_GAP_MS: i64 = 30_000; // collapse bursts (manual refreshes, replays)

pub struct HistoryLog(pub Mutex<Vec<Sample>>);

fn history_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("history.json"))
}

pub fn load(app: &AppHandle) -> Vec<Sample> {
    let Some(path) = history_path(app) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_log(&text, &path.with_file_name("history-legacy.bak.json"))
}

/// Parse the log, converting samples from older builds. The first time any
/// conversion happens the untouched file is kept at `backup` — this is the
/// user's only record of usage, so the pre-migration bytes stay recoverable.
fn parse_log(text: &str, backup: &Path) -> Vec<Sample> {
    let Ok(raw) = serde_json::from_str::<Vec<RawSample>>(text) else {
        return Vec::new();
    };
    if raw.iter().any(RawSample::is_legacy) && !backup.exists() {
        crate::state::write_atomic(backup, text);
    }
    raw.into_iter().map(RawSample::normalize).collect()
}

pub fn save(app: &AppHandle) {
    let Some(path) = history_path(app) else {
        return;
    };
    let Some(log) = app.try_state::<HistoryLog>() else {
        return;
    };
    let json = {
        let samples = log.0.lock().unwrap();
        serde_json::to_string(&*samples).unwrap()
    };
    crate::state::write_atomic(&path, &json);
}

/// Append a poll result, mirroring the frontend's sampling rules: error
/// snapshots and near-duplicate fetches are dropped, then the log is pruned.
pub fn record(samples: &mut Vec<Sample>, snapshot: &UsageSnapshot, now_ms: i64) -> bool {
    if snapshot.status != "ok" {
        return false;
    }
    let ms = if snapshot.fetched_at != 0 {
        snapshot.fetched_at
    } else {
        now_ms
    };
    if let Some(last) = samples.last() {
        if ms - last.ms < MIN_GAP_MS {
            return false;
        }
    }
    // Every window the poll reported, scoped ones included: the graph only
    // draws two of them today, but the data can't be recovered later. Windows
    // carried over from an earlier poll are skipped — a last known value is not
    // a new sample (see `usage::carry_missing_windows`).
    let w = snapshot
        .windows
        .iter()
        .filter(|win| !win.stale)
        .map(|win| {
            (
                win.id.clone(),
                WindowSample {
                    pct: win.utilization,
                    reset: win.reset_at.map(|s| s * 1000),
                },
            )
        })
        .collect();
    samples.push(Sample { ms, w });
    prune(samples, now_ms);
    true
}

/// One-time migration of the pre-backend localStorage log: accept only
/// samples older than everything already recorded here, so a re-run (or a
/// second webview instance) can never interleave duplicates.
pub fn import(samples: &mut Vec<Sample>, mut imported: Vec<Sample>, now_ms: i64) {
    let cutoff = samples.first().map(|s| s.ms).unwrap_or(i64::MAX);
    imported.retain(|s| s.ms < cutoff && s.ms > 0);
    if imported.is_empty() {
        return;
    }
    imported.sort_by_key(|s| s.ms);
    imported.append(samples);
    *samples = imported;
    prune(samples, now_ms);
}

fn prune(samples: &mut Vec<Sample>, now_ms: i64) {
    let mut kept: Vec<Sample> = Vec::with_capacity(samples.len());
    for s in samples.drain(..) {
        let age = now_ms - s.ms;
        if age > MAX_AGE_MS {
            continue;
        }
        if age > DENSE_AGE_MS {
            if let Some(last) = kept.last() {
                if s.ms - last.ms < SPARSE_GAP_MS {
                    continue;
                }
            }
        }
        kept.push(s);
    }
    *samples = kept;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{LimitWindow, UsageSnapshot};
    use std::sync::atomic::{AtomicU64, Ordering};

    const MIN: i64 = 60_000;

    fn snapshot(fetched_at: i64, five: f64, five_reset: Option<i64>) -> UsageSnapshot {
        UsageSnapshot {
            status: "ok".into(),
            source: Some("oauth".into()),
            fetched_at,
            windows: vec![LimitWindow {
                id: ID_SESSION.into(),
                label: "5h".into(),
                utilization: five,
                reset_at: five_reset,
                stale: false,
            }],
            error: None,
        }
    }

    fn sample(ms: i64, five: f64) -> Sample {
        Sample {
            ms,
            w: BTreeMap::from([(
                ID_SESSION.to_string(),
                WindowSample {
                    pct: five,
                    reset: None,
                },
            )]),
        }
    }

    /// A private directory for one test's files.
    fn temp_dir() -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tokometer-history-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn records_ok_snapshots_with_reset_times_in_ms() {
        let mut log = Vec::new();
        assert!(record(
            &mut log,
            &snapshot(1_000_000, 40.0, Some(18_000)),
            1_000_000
        ));
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].w[ID_SESSION].pct, 40.0);
        assert_eq!(log[0].w[ID_SESSION].reset, Some(18_000_000));
        assert!(!log[0].w.contains_key(ID_WEEKLY_ALL));
    }

    #[test]
    fn records_every_window_including_scoped_ones() {
        let mut log = Vec::new();
        let mut snap = snapshot(MIN, 19.0, None);
        snap.windows.push(LimitWindow {
            id: "weekly_scoped:fable".into(),
            label: "Fable".into(),
            utilization: 21.0,
            reset_at: Some(18_000),
            stale: false,
        });
        record(&mut log, &snap, MIN);
        assert_eq!(log[0].w["weekly_scoped:fable"].pct, 21.0);
        assert_eq!(log[0].w["weekly_scoped:fable"].reset, Some(18_000_000));
    }

    #[test]
    fn skips_windows_carried_over_from_an_earlier_poll() {
        let mut log = Vec::new();
        let mut snap = snapshot(MIN, 19.0, None);
        snap.windows.push(LimitWindow {
            id: "weekly_scoped:fable".into(),
            label: "Fable".into(),
            utilization: 21.0,
            reset_at: Some(18_000),
            stale: true,
        });
        assert!(record(&mut log, &snap, MIN));
        assert_eq!(log[0].w.keys().collect::<Vec<_>>(), [ID_SESSION]);
    }

    #[test]
    fn drops_error_snapshots_and_near_duplicates() {
        let mut log = Vec::new();
        let mut err = snapshot(0, 0.0, None);
        err.status = "error".into();
        assert!(!record(&mut log, &err, 0));
        assert!(record(&mut log, &snapshot(MIN, 10.0, None), MIN));
        // A startup replay 5s later is collapsed.
        assert!(!record(
            &mut log,
            &snapshot(MIN + 5_000, 10.0, None),
            MIN + 5_000
        ));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn prunes_old_samples_to_five_minute_spacing() {
        let mut log = Vec::new();
        for i in 0..10 {
            record(&mut log, &snapshot(i * MIN, i as f64, None), i * MIN);
        }
        let later = 7 * 60 * MIN;
        record(&mut log, &snapshot(later, 50.0, None), later);
        let old: Vec<_> = log.iter().filter(|s| s.ms < 10 * MIN).collect();
        assert!(old.len() < 10);
        for pair in old.windows(2) {
            assert!(pair[1].ms - pair[0].ms >= SPARSE_GAP_MS);
        }
    }

    #[test]
    fn drops_samples_older_than_the_cap() {
        let mut log = Vec::new();
        record(&mut log, &snapshot(0, 10.0, None), 0);
        let later = MAX_AGE_MS + MIN;
        record(&mut log, &snapshot(later, 20.0, None), later);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].ms, later);
    }

    #[test]
    fn import_prepends_only_samples_older_than_the_log() {
        let mut log = Vec::new();
        record(&mut log, &snapshot(100 * MIN, 30.0, None), 100 * MIN);
        let legacy = vec![
            sample(10 * MIN, 5.0),
            // Overlaps the backend log: must not interleave.
            sample(200 * MIN, 9.0),
        ];
        import(&mut log, legacy, 100 * MIN);
        assert_eq!(
            log.iter().map(|s| s.ms).collect::<Vec<_>>(),
            vec![10 * MIN, 100 * MIN]
        );
    }

    #[test]
    fn import_into_an_empty_log_keeps_everything_sorted() {
        let mut log = Vec::new();
        let legacy = vec![sample(20 * MIN, 9.0), sample(10 * MIN, 5.0)];
        import(&mut log, legacy, 30 * MIN);
        assert_eq!(
            log.iter().map(|s| s.ms).collect::<Vec<_>>(),
            vec![10 * MIN, 20 * MIN]
        );
    }

    // --- on-disk migration ---------------------------------------------------

    #[test]
    fn converts_samples_from_older_builds_into_the_keyed_map() {
        let dir = temp_dir();
        let text = r#"[{"ms":1000,"five":40,"week":12,"fiveReset":18000000,"weekReset":99000000}]"#;
        let log = parse_log(text, &dir.join("history-legacy.bak.json"));
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].w[ID_SESSION].pct, 40.0);
        assert_eq!(log[0].w[ID_SESSION].reset, Some(18_000_000));
        assert_eq!(log[0].w[ID_WEEKLY_ALL].pct, 12.0);
        assert_eq!(log[0].w[ID_WEEKLY_ALL].reset, Some(99_000_000));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_a_log_holding_both_shapes() {
        let dir = temp_dir();
        let text = r#"[
            {"ms":1000,"five":40,"week":null},
            {"ms":2000,"w":{"session":{"pct":41,"reset":null},
                            "weekly_scoped:fable":{"pct":21,"reset":18000000}}}
        ]"#;
        let log = parse_log(text, &dir.join("history-legacy.bak.json"));
        assert_eq!(log[0].w.keys().collect::<Vec<_>>(), [ID_SESSION]);
        assert_eq!(log[1].w["weekly_scoped:fable"].pct, 21.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backs_the_file_up_once_when_it_needed_converting() {
        let dir = temp_dir();
        let backup = dir.join("history-legacy.bak.json");
        let text = r#"[{"ms":1000,"five":40,"week":12}]"#;
        parse_log(text, &backup);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), text);
        // A later load must not overwrite the original with converted data.
        parse_log(r#"[{"ms":2000,"five":50,"week":13}]"#, &backup);
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), text);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writes_no_backup_for_a_log_that_is_already_keyed() {
        let dir = temp_dir();
        let backup = dir.join("history-legacy.bak.json");
        parse_log(
            r#"[{"ms":1000,"w":{"session":{"pct":40,"reset":null}}}]"#,
            &backup,
        );
        assert!(!backup.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tolerates_an_unreadable_log_without_touching_it() {
        let dir = temp_dir();
        let backup = dir.join("history-legacy.bak.json");
        assert!(parse_log("not json", &backup).is_empty());
        assert!(!backup.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn serializes_only_the_keyed_shape() {
        let v = serde_json::to_value(sample(1000, 40.0)).unwrap();
        assert_eq!(v["w"][ID_SESSION]["pct"], serde_json::json!(40.0));
        assert!(v.get("five").is_none());
    }
}
