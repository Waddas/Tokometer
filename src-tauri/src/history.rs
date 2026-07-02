//! Backend usage-history log behind the graph. Lives next to state.json so it
//! survives WebView data clears, reinstalls, and the dev/prod origin split
//! that used to strand the frontend's localStorage copy.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::usage::UsageSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// unix epoch ms
    pub ms: i64,
    /// 0-100 percent, absent when the poll lacked that window
    pub five: Option<f64>,
    pub week: Option<f64>,
    /// each window's reset time (epoch ms); absent on samples from older builds
    #[serde(default)]
    pub five_reset: Option<i64>,
    #[serde(default)]
    pub week_reset: Option<i64>,
}

const MAX_AGE_MS: i64 = 15 * 86_400_000; // current 7-day window plus the previous one
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
    history_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
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
    let reset_ms = |w: &Option<crate::usage::UsageWindow>| {
        w.as_ref().and_then(|w| w.reset_at).map(|s| s * 1000)
    };
    samples.push(Sample {
        ms,
        five: snapshot.five_hour.as_ref().map(|w| w.utilization),
        week: snapshot.seven_day.as_ref().map(|w| w.utilization),
        five_reset: reset_ms(&snapshot.five_hour),
        week_reset: reset_ms(&snapshot.seven_day),
    });
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
    use crate::usage::{UsageSnapshot, UsageWindow};

    const MIN: i64 = 60_000;

    fn snapshot(fetched_at: i64, five: f64, five_reset: Option<i64>) -> UsageSnapshot {
        UsageSnapshot {
            status: "ok".into(),
            source: Some("oauth".into()),
            fetched_at,
            five_hour: Some(UsageWindow {
                utilization: five,
                reset_at: five_reset,
            }),
            seven_day: None,
            five_hour_status: None,
            error: None,
        }
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
        assert_eq!(log[0].five, Some(40.0));
        assert_eq!(log[0].five_reset, Some(18_000_000));
        assert_eq!(log[0].week, None);
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
            Sample {
                ms: 10 * MIN,
                five: Some(5.0),
                week: None,
                five_reset: None,
                week_reset: None,
            },
            // Overlaps the backend log: must not interleave.
            Sample {
                ms: 200 * MIN,
                five: Some(9.0),
                week: None,
                five_reset: None,
                week_reset: None,
            },
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
        let legacy = vec![
            Sample {
                ms: 20 * MIN,
                five: Some(9.0),
                week: None,
                five_reset: None,
                week_reset: None,
            },
            Sample {
                ms: 10 * MIN,
                five: Some(5.0),
                week: None,
                five_reset: None,
                week_reset: None,
            },
        ];
        import(&mut log, legacy, 30 * MIN);
        assert_eq!(
            log.iter().map(|s| s.ms).collect::<Vec<_>>(),
            vec![10 * MIN, 20 * MIN]
        );
    }
}
