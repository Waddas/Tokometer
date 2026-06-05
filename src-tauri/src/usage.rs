use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    /// 0–100 percent.
    pub utilization: f64,
    /// Unix epoch seconds.
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub status: String, // "ok" | "error"
    pub source: Option<String>, // "oauth" | "messages"
    pub fetched_at: i64, // unix epoch ms
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    pub five_hour_status: Option<String>, // "allowed" | "limited"
    pub error: Option<String>,
}

impl UsageSnapshot {
    pub fn ok(
        source: &str,
        five_hour: UsageWindow,
        seven_day: UsageWindow,
        five_hour_status: Option<String>,
    ) -> Self {
        Self {
            status: "ok".into(),
            source: Some(source.into()),
            fetched_at: now_ms(),
            five_hour: Some(five_hour),
            seven_day: Some(seven_day),
            five_hour_status,
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            status: "error".into(),
            source: None,
            fetched_at: now_ms(),
            five_hour: None,
            seven_day: None,
            five_hour_status: None,
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

fn window_from_value(v: &Value) -> Option<UsageWindow> {
    // Verified live: `utilization` is 0–100 percent, `resets_at` is RFC 3339.
    let raw = v.get("utilization")?.as_f64()?;
    let reset_at = v.get("resets_at").and_then(value_to_epoch_secs);
    Some(UsageWindow { utilization: raw.clamp(0.0, 100.0), reset_at })
}

/// Parse the `GET /api/oauth/usage` response body.
pub fn from_oauth_body(v: &Value) -> Option<UsageSnapshot> {
    let five = window_from_value(v.get("five_hour")?)?;
    let seven = window_from_value(v.get("seven_day")?)?;
    Some(UsageSnapshot::ok("oauth", five, seven, None))
}

/// Parse the rate-limit headers of a `/v1/messages` response
/// (fallback path — mirrors Clawdmeter's daemon; utilization is a 0–1 fraction).
pub fn from_ratelimit_headers(headers: &reqwest::header::HeaderMap) -> Option<UsageSnapshot> {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let epoch = |name: &str| {
        get(name).and_then(|s| s.parse::<i64>().ok().or_else(|| parse_rfc3339_to_epoch(s)))
    };

    let five_util: f64 = get("anthropic-ratelimit-unified-5h-utilization")?.parse().ok()?;
    let seven_util: f64 = get("anthropic-ratelimit-unified-7d-utilization")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    Some(UsageSnapshot::ok(
        "messages",
        UsageWindow {
            utilization: (five_util * 100.0).clamp(0.0, 100.0),
            reset_at: epoch("anthropic-ratelimit-unified-5h-reset"),
        },
        UsageWindow {
            utilization: (seven_util * 100.0).clamp(0.0, 100.0),
            reset_at: epoch("anthropic-ratelimit-unified-7d-reset"),
        },
        get("anthropic-ratelimit-unified-5h-status").map(str::to_string),
    ))
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
        assert_eq!(parse_rfc3339_to_epoch("2026-06-04T18:00:00Z"), Some(1_780_596_000));
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
        assert_eq!(value_to_epoch_secs(&json!(1_780_682_400_i64)), Some(1_780_682_400));
    }

    #[test]
    fn converts_large_numbers_from_millis_to_seconds() {
        // > 1e11 is assumed to be milliseconds.
        assert_eq!(value_to_epoch_secs(&json!(1_780_682_400_000_i64)), Some(1_780_682_400));
    }

    #[test]
    fn parses_rfc3339_strings_in_values() {
        assert_eq!(value_to_epoch_secs(&json!("2026-06-04T18:00:00Z")), Some(1_780_596_000));
    }

    #[test]
    fn returns_none_for_unparseable_values() {
        assert_eq!(value_to_epoch_secs(&json!("not a date")), None);
        assert_eq!(value_to_epoch_secs(&json!(true)), None);
    }

    // --- window_from_value ---------------------------------------------------

    #[test]
    fn builds_window_with_clamped_utilization() {
        let w = window_from_value(&json!({ "utilization": 150.0 })).unwrap();
        assert_eq!(w.utilization, 100.0);
        assert_eq!(w.reset_at, None);

        let w = window_from_value(&json!({ "utilization": -10.0 })).unwrap();
        assert_eq!(w.utilization, 0.0);
    }

    #[test]
    fn reads_reset_time_when_present() {
        let w = window_from_value(&json!({
            "utilization": 25.0,
            "resets_at": "2026-06-04T18:00:00Z",
        }))
        .unwrap();
        assert_eq!(w.utilization, 25.0);
        assert_eq!(w.reset_at, Some(1_780_596_000));
    }

    #[test]
    fn returns_none_without_utilization() {
        assert!(window_from_value(&json!({ "resets_at": "2026-06-04T18:00:00Z" })).is_none());
    }

    // --- from_oauth_body -----------------------------------------------------

    #[test]
    fn parses_a_complete_oauth_body() {
        let body = json!({
            "five_hour": { "utilization": 40.0, "resets_at": "2026-06-04T18:00:00Z" },
            "seven_day": { "utilization": 12.5, "resets_at": "2026-06-10T00:00:00Z" },
        });
        let snap = from_oauth_body(&body).unwrap();
        assert_eq!(snap.status, "ok");
        assert_eq!(snap.source.as_deref(), Some("oauth"));
        assert_eq!(snap.five_hour.unwrap().utilization, 40.0);
        assert_eq!(snap.seven_day.unwrap().utilization, 12.5);
        assert!(snap.five_hour_status.is_none());
    }

    #[test]
    fn rejects_an_oauth_body_missing_a_window() {
        let body = json!({ "five_hour": { "utilization": 40.0 } });
        assert!(from_oauth_body(&body).is_none());
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
            ("anthropic-ratelimit-unified-5h-status", "allowed"),
        ]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.source.as_deref(), Some("messages"));
        assert!((snap.five_hour.as_ref().unwrap().utilization - 42.0).abs() < 1e-9);
        assert_eq!(snap.five_hour.unwrap().reset_at, Some(1_780_596_000));
        assert!((snap.seven_day.unwrap().utilization - 10.0).abs() < 1e-9);
        assert_eq!(snap.five_hour_status.as_deref(), Some("allowed"));
    }

    #[test]
    fn defaults_seven_day_utilization_to_zero_when_absent() {
        let h = headers(&[("anthropic-ratelimit-unified-5h-utilization", "0.5")]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.seven_day.unwrap().utilization, 0.0);
    }

    #[test]
    fn clamps_overscale_header_utilization() {
        let h = headers(&[("anthropic-ratelimit-unified-5h-utilization", "1.5")]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.five_hour.unwrap().utilization, 100.0);
    }

    #[test]
    fn parses_rfc3339_reset_headers_too() {
        let h = headers(&[
            ("anthropic-ratelimit-unified-5h-utilization", "0.5"),
            ("anthropic-ratelimit-unified-5h-reset", "2026-06-04T18:00:00Z"),
        ]);
        let snap = from_ratelimit_headers(&h).unwrap();
        assert_eq!(snap.five_hour.unwrap().reset_at, Some(1_780_596_000));
    }

    #[test]
    fn returns_none_without_the_required_five_hour_header() {
        assert!(from_ratelimit_headers(&headers(&[])).is_none());
    }

    // --- UsageSnapshot constructors ------------------------------------------

    #[test]
    fn ok_snapshot_has_ok_status_and_no_error() {
        let w = || UsageWindow { utilization: 1.0, reset_at: None };
        let snap = UsageSnapshot::ok("oauth", w(), w(), Some("allowed".into()));
        assert_eq!(snap.status, "ok");
        assert!(snap.error.is_none());
        assert_eq!(snap.five_hour_status.as_deref(), Some("allowed"));
    }

    #[test]
    fn error_snapshot_carries_the_message_and_no_windows() {
        let snap = UsageSnapshot::error("boom".into());
        assert_eq!(snap.status, "error");
        assert_eq!(snap.error.as_deref(), Some("boom"));
        assert!(snap.source.is_none());
        assert!(snap.five_hour.is_none());
        assert!(snap.seven_day.is_none());
    }

    #[test]
    fn snapshot_serializes_to_camel_case_for_the_frontend() {
        let snap = UsageSnapshot::ok(
            "oauth",
            UsageWindow { utilization: 5.0, reset_at: Some(10) },
            UsageWindow { utilization: 6.0, reset_at: None },
            None,
        );
        let v = serde_json::to_value(&snap).unwrap();
        assert!(v.get("fiveHour").is_some());
        assert!(v.get("sevenDay").is_some());
        assert!(v.get("fetchedAt").is_some());
        assert_eq!(v["fiveHour"]["resetAt"], json!(10));
    }
}
