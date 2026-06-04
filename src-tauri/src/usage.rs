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
