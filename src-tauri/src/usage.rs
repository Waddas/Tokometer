use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Well-known window ids: the 5-hour session and the all-model week. The graph,
/// the rate tracker and the tray icon look these up by name, so they are a
/// contract with the frontend (`SESSION_ID`/`WEEKLY_ALL_ID` in src/api.ts) and
/// with the history log's keys.
pub const ID_SESSION: &str = "session";
pub const ID_WEEKLY_ALL: &str = "weekly_all";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWindow {
    /// Stable id: the API's `kind`, plus a slug of the scope's model name when
    /// scoped — "session", "weekly_all", "weekly_scoped:fable".
    pub id: String,
    /// Tile label: "5h", "7d", or the scope's display name ("Fable").
    pub label: String,
    /// 0–100 percent.
    pub utilization: f64,
    /// Unix epoch seconds.
    pub reset_at: Option<i64>,
    /// A last known value carried over because this poll couldn't observe the
    /// window (see `carry_missing_windows`). Omitted from JSON when false, so
    /// live windows and older state.json files serialize identically.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub status: String,         // "ok" | "error"
    pub source: Option<String>, // "oauth" | "messages"
    pub fetched_at: i64,        // unix epoch ms
    /// Every limit window the poll reported, in the order the API listed them.
    #[serde(default)]
    pub windows: Vec<LimitWindow>,
    pub error: Option<String>,
}

impl UsageSnapshot {
    pub fn ok(source: &str, windows: Vec<LimitWindow>) -> Self {
        Self {
            status: "ok".into(),
            source: Some(source.into()),
            fetched_at: now_ms(),
            windows,
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            status: "error".into(),
            source: None,
            fetched_at: now_ms(),
            windows: Vec::new(),
            error: Some(message),
        }
    }
}

fn value_to_epoch_secs(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_f64() {
        let n = n as i64;
        // Disambiguate seconds vs milliseconds.
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    v.as_str().and_then(parse_rfc3339_to_epoch)
}

fn window_from_value(v: &Value, id: &str, label: &str) -> Option<LimitWindow> {
    // Verified live: `utilization` is 0–100 percent, `resets_at` is RFC 3339.
    let raw = v.get("utilization")?.as_f64()?;
    Some(LimitWindow {
        id: id.into(),
        label: label.into(),
        utilization: raw.clamp(0.0, 100.0),
        reset_at: v.get("resets_at").and_then(value_to_epoch_secs),
        stale: false,
    })
}

/// A scoped limit's model name as an id fragment: lowercase, everything else
/// dashed, so ids stay stable and file/JSON safe ("Claude Fable" → "claude-fable").
fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// One entry of the `limits` array. Unknown kinds still get a window (labelled
/// with the kind itself) — that is how a new Anthropic limit auto-appears.
fn limit_from_value(v: &Value) -> Option<LimitWindow> {
    let kind = v.get("kind")?.as_str()?;
    let percent = v.get("percent")?.as_f64()?;
    let model = v
        .get("scope")
        .and_then(|s| s.get("model"))
        .and_then(|m| m.get("display_name"))
        .and_then(Value::as_str);
    Some(LimitWindow {
        id: match model {
            Some(name) => format!("{kind}:{}", slug(name)),
            None => kind.to_string(),
        },
        label: match (model, kind) {
            (Some(name), _) => name.to_string(),
            (None, ID_SESSION) => "5h".into(),
            (None, ID_WEEKLY_ALL) => "7d".into(),
            (None, kind) => kind.to_string(),
        },
        utilization: percent.clamp(0.0, 100.0),
        reset_at: v.get("resets_at").and_then(value_to_epoch_secs),
        stale: false,
    })
}

/// Parse the `GET /api/oauth/usage` response body: the `limits` array when it
/// carries anything, else the two fixed top-level windows older responses had.
/// One parsed window is enough to call the poll a success.
pub fn from_oauth_body(v: &Value) -> Option<UsageSnapshot> {
    let windows: Vec<LimitWindow> = match v.get("limits").and_then(Value::as_array) {
        Some(limits) if !limits.is_empty() => limits.iter().filter_map(limit_from_value).collect(),
        _ => [
            ("five_hour", ID_SESSION, "5h"),
            ("seven_day", ID_WEEKLY_ALL, "7d"),
        ]
        .into_iter()
        .filter_map(|(key, id, label)| window_from_value(v.get(key)?, id, label))
        .collect(),
    };
    (!windows.is_empty()).then(|| UsageSnapshot::ok("oauth", windows))
}

/// Parse the rate-limit headers of a `/v1/messages` response
/// (fallback path — mirrors Clawdmeter's daemon; utilization is a 0–1 fraction).
pub fn from_ratelimit_headers(headers: &reqwest::header::HeaderMap) -> Option<UsageSnapshot> {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let epoch = |name: &str| {
        get(name).and_then(|s| s.parse::<i64>().ok().or_else(|| parse_rfc3339_to_epoch(s)))
    };

    let five_util: f64 = get("anthropic-ratelimit-unified-5h-utilization")?
        .parse()
        .ok()?;
    let seven_util: f64 = get("anthropic-ratelimit-unified-7d-utilization")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    Some(UsageSnapshot::ok(
        "messages",
        vec![
            LimitWindow {
                id: ID_SESSION.into(),
                label: "5h".into(),
                utilization: (five_util * 100.0).clamp(0.0, 100.0),
                reset_at: epoch("anthropic-ratelimit-unified-5h-reset"),
                stale: false,
            },
            LimitWindow {
                id: ID_WEEKLY_ALL.into(),
                label: "7d".into(),
                utilization: (seven_util * 100.0).clamp(0.0, 100.0),
                reset_at: epoch("anthropic-ratelimit-unified-7d-reset"),
                stale: false,
            },
        ],
    ))
}

/// Keep the windows a fallback probe cannot see. `/v1/messages` headers only
/// report the session and the all-model week, so scoped limits would blink out
/// of the widget on every probe-sourced poll; instead their last known values
/// are appended, flagged stale so the tiles show them dimmed. An oauth poll
/// sees every window, so its snapshot is authoritative and never carried into.
pub fn carry_missing_windows(current: &mut UsageSnapshot, previous: &UsageSnapshot) {
    if current.status != "ok"
        || current.source.as_deref() != Some("messages")
        || previous.status != "ok"
    {
        return;
    }
    let carried: Vec<LimitWindow> = previous
        .windows
        .iter()
        .filter(|w| !current.windows.iter().any(|c| c.id == w.id))
        .map(|w| LimitWindow {
            stale: true,
            ..w.clone()
        })
        .collect();
    current.windows.extend(carried);
}

/// Tiny RFC 3339 → epoch-seconds parser ("2026-06-04T18:00:00Z", optional
/// fractional seconds / numeric offset). Avoids pulling in a date crate.
fn parse_rfc3339_to_epoch(s: &str) -> Option<i64> {
    if s.len() < 20 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;

    // Days from civil (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    let mut epoch = days * 86400 + hour * 3600 + min * 60 + sec;
    // Apply a numeric UTC offset if present after the seconds field.
    let rest = &s[19..];
    if let Some(pos) = rest.find(['+', '-']) {
        let off = &rest[pos..];
        let sign: i64 = if off.starts_with('-') { -1 } else { 1 };
        let oh: i64 = off.get(1..3)?.parse().ok()?;
        let om: i64 = off.get(4..6)?.parse().ok()?;
        epoch -= sign * (oh * 3600 + om * 60);
    }
    Some(epoch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderMap;
    use serde_json::json;

    // --- parse_rfc3339_to_epoch ---------------------------------------------

    #[test]
    fn parses_basic_utc_timestamp() {
        // 2026-06-04T18:00:00Z == 1780596000 (verified against a known epoch).
        assert_eq!(
            parse_rfc3339_to_epoch("2026-06-04T18:00:00Z"),
            Some(1_780_596_000)
        );
    }

    #[test]
    fn parses_the_unix_epoch_itself() {
        assert_eq!(parse_rfc3339_to_epoch("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn ignores_fractional_seconds() {
        let a = parse_rfc3339_to_epoch("2026-06-04T18:00:00.123Z");
        assert_eq!(a, parse_rfc3339_to_epoch("2026-06-04T18:00:00Z"));
    }

    #[test]
    fn applies_positive_numeric_offset() {
        // +02:00 is two hours ahead of UTC, so the epoch is two hours earlier.
        let utc = parse_rfc3339_to_epoch("2026-06-04T18:00:00Z").unwrap();
        let offset = parse_rfc3339_to_epoch("2026-06-04T18:00:00+02:00").unwrap();
        assert_eq!(offset, utc - 2 * 3600);
    }

    #[test]
    fn applies_negative_numeric_offset() {
        let utc = parse_rfc3339_to_epoch("2026-06-04T18:00:00Z").unwrap();
        let offset = parse_rfc3339_to_epoch("2026-06-04T18:00:00-05:00").unwrap();
        assert_eq!(offset, utc + 5 * 3600);
    }

    #[test]
    fn rejects_strings_that_are_too_short() {
        assert_eq!(parse_rfc3339_to_epoch("2026-06-04"), None);
    }

    #[test]
    fn rejects_non_numeric_fields() {
        assert_eq!(parse_rfc3339_to_epoch("20X6-06-04T18:00:00Z"), None);
    }

    // --- value_to_epoch_secs -------------------------------------------------

    #[test]
    fn treats_small_numbers_as_seconds() {
        assert_eq!(
            value_to_epoch_secs(&json!(1_780_682_400_i64)),
            Some(1_780_682_400)
        );
    }

    #[test]
    fn converts_large_numbers_from_millis_to_seconds() {
        // > 1e11 is assumed to be milliseconds.
        assert_eq!(
            value_to_epoch_secs(&json!(1_780_682_400_000_i64)),
            Some(1_780_682_400)
        );
    }

    #[test]
    fn parses_rfc3339_strings_in_values() {
        assert_eq!(
            value_to_epoch_secs(&json!("2026-06-04T18:00:00Z")),
            Some(1_780_596_000)
        );
    }

    #[test]
    fn returns_none_for_unparseable_values() {
        assert_eq!(value_to_epoch_secs(&json!("not a date")), None);
        assert_eq!(value_to_epoch_secs(&json!(true)), None);
    }

    // --- window_from_value ---------------------------------------------------

    #[test]
    fn builds_window_with_clamped_utilization() {
        let w = window_from_value(&json!({ "utilization": 150.0 }), ID_SESSION, "5h").unwrap();
        assert_eq!(w.id, "session");
        assert_eq!(w.label, "5h");
        assert_eq!(w.utilization, 100.0);
        assert_eq!(w.reset_at, None);

        let w = window_from_value(&json!({ "utilization": -10.0 }), ID_SESSION, "5h").unwrap();
        assert_eq!(w.utilization, 0.0);
    }

    #[test]
    fn reads_reset_time_when_present() {
        let w = window_from_value(
            &json!({
                "utilization": 25.0,
                "resets_at": "2026-06-04T18:00:00Z",
            }),
            ID_WEEKLY_ALL,
            "7d",
        )
        .unwrap();
        assert_eq!(w.utilization, 25.0);
        assert_eq!(w.reset_at, Some(1_780_596_000));
    }

    #[test]
    fn returns_none_without_utilization() {
        let v = json!({ "resets_at": "2026-06-04T18:00:00Z" });
        assert!(window_from_value(&v, ID_SESSION, "5h").is_none());
    }

    // --- well-known ids ------------------------------------------------------

    #[test]
    fn well_known_ids_match_the_frontend() {
        // Contract with src/api.ts `SESSION_ID`/`WEEKLY_ALL_ID`, the graph's
        // mode keys and the history log's sample keys — changing one side must
        // change the other.
        assert_eq!(ID_SESSION, "session");
        assert_eq!(ID_WEEKLY_ALL, "weekly_all");
    }

    // --- limits array --------------------------------------------------------

    /// The live body, trimmed to the keys that matter (verified 2026-07-28).
    fn limits_body() -> Value {
        json!({
            "five_hour": { "utilization": 19.0, "resets_at": "2026-07-28T13:39:59.032477+01:00" },
            "seven_day": { "utilization": 20.0, "resets_at": "2026-07-31T07:00:00.032502+01:00" },
            "limits": [
                { "kind": "session", "group": "session", "percent": 19, "severity": "normal",
                  "resets_at": "2026-07-28T13:39:59.032477+01:00", "scope": null, "is_active": false },
                { "kind": "weekly_all", "group": "weekly", "percent": 20, "severity": "normal",
                  "resets_at": "2026-07-31T07:00:00.032502+01:00", "scope": null, "is_active": false },
                { "kind": "weekly_scoped", "group": "weekly", "percent": 21, "severity": "normal",
                  "resets_at": "2026-07-31T07:00:00.032879+01:00",
                  "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null },
                  "is_active": true }
            ]
        })
    }

    #[test]
    fn parses_the_limits_array_in_order() {
        let snap = from_oauth_body(&limits_body()).unwrap();
        assert_eq!(snap.status, "ok");
        assert_eq!(snap.source.as_deref(), Some("oauth"));
        assert_eq!(
            snap.windows
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly_all", "weekly_scoped:fable"]
        );
        assert_eq!(
            snap.windows
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>(),
            ["5h", "7d", "Fable"]
        );
        assert_eq!(
            snap.windows
                .iter()
                .map(|w| w.utilization)
                .collect::<Vec<_>>(),
            [19.0, 20.0, 21.0]
        );
        // +01:00, so the epoch is an hour earlier than the wall-clock reading.
        assert_eq!(
            snap.windows[0].reset_at,
            parse_rfc3339_to_epoch("2026-07-28T12:39:59Z")
        );
    }

    #[test]
    fn the_limits_array_wins_over_the_legacy_top_level_windows() {
        let mut body = limits_body();
        body["five_hour"]["utilization"] = json!(99.0);
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(snap.windows[0].utilization, 19.0);
    }

    #[test]
    fn labels_an_unknown_unscoped_kind_with_its_kind() {
        let body = json!({ "limits": [{ "kind": "monthly_all", "percent": 4 }] });
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(snap.windows[0].id, "monthly_all");
        assert_eq!(snap.windows[0].label, "monthly_all");
    }

    #[test]
    fn slugs_scoped_model_names_into_ids() {
        assert_eq!(slug("Fable"), "fable");
        assert_eq!(slug("Claude Opus 4.5"), "claude-opus-4-5");
    }

    #[test]
    fn skips_limit_entries_without_a_kind_or_percent() {
        let body = json!({
            "limits": [
                { "percent": 10 },
                { "kind": "session" },
                { "kind": "weekly_all", "percent": 20 },
            ]
        });
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].id, "weekly_all");
    }

    #[test]
    fn clamps_limit_percentages() {
        let body = json!({ "limits": [{ "kind": "session", "percent": 140 }] });
        assert_eq!(
            from_oauth_body(&body).unwrap().windows[0].utilization,
            100.0
        );
    }

    // --- from_oauth_body (legacy shape) --------------------------------------

    #[test]
    fn parses_a_body_without_a_limits_array() {
        let body = json!({
            "five_hour": { "utilization": 40.0, "resets_at": "2026-06-04T18:00:00Z" },
            "seven_day": { "utilization": 12.5, "resets_at": "2026-06-10T00:00:00Z" },
        });
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(
            snap.windows
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly_all"]
        );
        assert_eq!(snap.windows[0].utilization, 40.0);
        assert_eq!(snap.windows[1].utilization, 12.5);
    }

    #[test]
    fn accepts_a_legacy_body_carrying_only_one_window() {
        let body = json!({ "five_hour": { "utilization": 40.0 } });
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].id, "session");
    }

    #[test]
    fn rejects_a_body_with_no_windows_at_all() {
        assert!(from_oauth_body(&json!({ "limits": [], "spend": {} })).is_none());
        assert!(from_oauth_body(&json!({ "seven_day_opus": null })).is_none());
    }

    // --- from_ratelimit_headers ----------------------------------------------

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        use reqwest::header::{HeaderName, HeaderValue};
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            let name = HeaderName::from_bytes(k.as_bytes()).unwrap();
            h.insert(name, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn parses_ratelimit_headers_scaling_fraction_to_percent() {
        let h = headers(&[
            ("anthropic-ratelimit-unified-5h-utilization", "0.42"),
            ("anthropic-ratelimit-unified-5h-reset", "1780596000"),
            ("anthropic-ratelimit-unified-7d-utilization", "0.1"),
        ]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.source.as_deref(), Some("messages"));
        assert_eq!(snap.windows[0].id, ID_SESSION);
        assert!((snap.windows[0].utilization - 42.0).abs() < 1e-9);
        assert_eq!(snap.windows[0].reset_at, Some(1_780_596_000));
        assert_eq!(snap.windows[1].id, ID_WEEKLY_ALL);
        assert!((snap.windows[1].utilization - 10.0).abs() < 1e-9);
    }

    #[test]
    fn defaults_seven_day_utilization_to_zero_when_absent() {
        let h = headers(&[("anthropic-ratelimit-unified-5h-utilization", "0.5")]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.windows[1].utilization, 0.0);
    }

    #[test]
    fn clamps_overscale_header_utilization() {
        let h = headers(&[("anthropic-ratelimit-unified-5h-utilization", "1.5")]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.windows[0].utilization, 100.0);
    }

    #[test]
    fn parses_rfc3339_reset_headers_too() {
        let h = headers(&[
            ("anthropic-ratelimit-unified-5h-utilization", "0.5"),
            (
                "anthropic-ratelimit-unified-5h-reset",
                "2026-06-04T18:00:00Z",
            ),
        ]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.windows[0].reset_at, Some(1_780_596_000));
    }

    #[test]
    fn returns_none_without_the_required_five_hour_header() {
        assert!(from_ratelimit_headers(&headers(&[])).is_none());
    }

    // --- UsageSnapshot constructors ------------------------------------------

    fn window(id: &str, utilization: f64) -> LimitWindow {
        LimitWindow {
            id: id.into(),
            label: "5h".into(),
            utilization,
            reset_at: Some(10),
            stale: false,
        }
    }

    #[test]
    fn ok_snapshot_has_ok_status_and_no_error() {
        let snap = UsageSnapshot::ok("oauth", vec![window(ID_SESSION, 1.0)]);
        assert_eq!(snap.status, "ok");
        assert!(snap.error.is_none());
        assert_eq!(snap.windows.len(), 1);
    }

    #[test]
    fn error_snapshot_carries_the_message_and_no_windows() {
        let snap = UsageSnapshot::error("boom".into());
        assert_eq!(snap.status, "error");
        assert_eq!(snap.error.as_deref(), Some("boom"));
        assert!(snap.source.is_none());
        assert!(snap.windows.is_empty());
    }

    #[test]
    fn snapshot_serializes_to_camel_case_for_the_frontend() {
        let snap = UsageSnapshot::ok("oauth", vec![window(ID_SESSION, 5.0)]);
        let v = serde_json::to_value(&snap).unwrap();
        assert!(v.get("fetchedAt").is_some());
        assert_eq!(v["windows"][0]["id"], json!("session"));
        assert_eq!(v["windows"][0]["resetAt"], json!(10));
        // A live window carries no `stale` key at all, so the persisted shape
        // is byte-identical to what builds before the carry-over wrote.
        assert!(v["windows"][0].get("stale").is_none());
    }

    #[test]
    fn a_carried_window_serializes_its_stale_flag() {
        let mut snap = UsageSnapshot::ok("messages", vec![window(ID_SESSION, 5.0)]);
        snap.windows[0].stale = true;
        let v = serde_json::to_value(&snap).unwrap();
        assert_eq!(v["windows"][0]["stale"], json!(true));
    }

    // --- carry_missing_windows -----------------------------------------------

    /// The previous oauth poll: both fixed windows plus a scoped one.
    fn previous_oauth() -> UsageSnapshot {
        let mut snap = UsageSnapshot::ok(
            "oauth",
            vec![
                window(ID_SESSION, 19.0),
                window(ID_WEEKLY_ALL, 20.0),
                window("weekly_scoped:fable", 21.0),
            ],
        );
        snap.windows[2].label = "Fable".into();
        snap.windows[2].reset_at = Some(99);
        snap
    }

    /// What the fallback probe can report: the two header-visible windows.
    fn probe_snapshot() -> UsageSnapshot {
        UsageSnapshot::ok(
            "messages",
            vec![window(ID_SESSION, 25.0), window(ID_WEEKLY_ALL, 22.0)],
        )
    }

    #[test]
    fn carries_a_missing_scoped_window_flagged_stale() {
        let mut snap = probe_snapshot();
        carry_missing_windows(&mut snap, &previous_oauth());
        assert_eq!(
            snap.windows
                .iter()
                .map(|w| w.id.as_str())
                .collect::<Vec<_>>(),
            ["session", "weekly_all", "weekly_scoped:fable"]
        );
        // The probe's own readings stay live; only the carried one is stale.
        assert_eq!(snap.windows[0].utilization, 25.0);
        assert!(!snap.windows[0].stale);
        let fable = &snap.windows[2];
        assert_eq!(fable.label, "Fable");
        assert_eq!(fable.utilization, 21.0);
        assert_eq!(fable.reset_at, Some(99));
        assert!(fable.stale);
    }

    #[test]
    fn never_carries_into_an_oauth_snapshot() {
        let mut snap = UsageSnapshot::ok("oauth", vec![window(ID_SESSION, 30.0)]);
        carry_missing_windows(&mut snap, &previous_oauth());
        assert_eq!(snap.windows.len(), 1);
    }

    #[test]
    fn carries_nothing_from_a_failed_previous_poll() {
        let mut snap = probe_snapshot();
        carry_missing_windows(&mut snap, &UsageSnapshot::error("boom".into()));
        assert_eq!(snap.windows.len(), 2);
    }

    #[test]
    fn chained_probe_polls_keep_carrying_the_window() {
        let mut first = probe_snapshot();
        carry_missing_windows(&mut first, &previous_oauth());
        let mut second = probe_snapshot();
        carry_missing_windows(&mut second, &first);
        assert_eq!(second.windows.len(), 3);
        assert_eq!(second.windows[2].id, "weekly_scoped:fable");
        assert_eq!(second.windows[2].utilization, 21.0);
        assert!(second.windows[2].stale);
    }

    #[test]
    fn a_snapshot_stored_by_an_older_build_deserializes_without_windows() {
        // A pre-upgrade state.json holds the old fixed-window shape; it must
        // still load (empty window list) and self-heal on the first poll.
        let v = json!({
            "status": "ok",
            "source": "oauth",
            "fetchedAt": 1,
            "fiveHour": { "utilization": 40.0, "resetAt": 2 },
            "error": null,
        });
        let snap: UsageSnapshot = serde_json::from_value(v).unwrap();
        assert!(snap.windows.is_empty());
    }
}
