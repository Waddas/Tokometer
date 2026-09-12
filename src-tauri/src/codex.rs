//! Read-only Codex OAuth usage. No CLI, app-server, token refresh or auth-file writes.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use crate::state::Provider;
use crate::usage::{LimitWindow, UsageSnapshot, ID_SESSION, ID_WEEKLY_ALL};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const SIGN_IN: &str =
    "Codex login unavailable — sign in to Codex with ChatGPT using file-based credential storage";

struct Credentials {
    token: String,
    account: String,
    scope: String,
}

fn claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?).ok()
}

fn nonempty(v: Option<&Value>) -> Option<&str> {
    v?.as_str().filter(|s| !s.trim().is_empty())
}

fn parse_credentials(text: &str) -> Result<Credentials, String> {
    let v: Value = serde_json::from_str(text).map_err(|_| SIGN_IN.to_string())?;
    if v.get("auth_mode")
        .and_then(Value::as_str)
        .is_some_and(|m| m != "chatgpt")
    {
        return Err(
            "Codex requires a ChatGPT login — API-key and other login modes are not supported"
                .into(),
        );
    }
    let t = &v["tokens"];
    let token = nonempty(t.get("access_token")).ok_or(SIGN_IN)?;
    let identity = nonempty(t.get("id_token")).and_then(claims);
    let access = claims(token);
    let account = nonempty(t.get("account_id"))
        .or_else(|| {
            identity
                .as_ref()
                .and_then(|v| nonempty(v["https://api.openai.com/auth"].get("chatgpt_account_id")))
        })
        .or_else(|| {
            access
                .as_ref()
                .and_then(|v| nonempty(v["https://api.openai.com/auth"].get("chatgpt_account_id")))
        })
        .ok_or(SIGN_IN)?;
    // JWT claims only partition local history; the unchanged token is authenticated by OpenAI.
    let subject = identity
        .as_ref()
        .and_then(|v| nonempty(v.get("sub")))
        .or_else(|| access.as_ref().and_then(|v| nonempty(v.get("sub"))))
        .ok_or("Codex account identity unavailable — sign in to Codex again")?;
    let digest = Sha256::digest(format!("{account}\0{subject}").as_bytes());
    Ok(Credentials {
        token: token.into(),
        account: account.into(),
        scope: format!("codex:{digest:x}"),
    })
}

fn auth_path() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return Ok(PathBuf::from(home).join("auth.json"));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or(SIGN_IN)?;
    Ok(PathBuf::from(home).join(".codex/auth.json"))
}

/// Detect local Codex use even if the saved login is expired or offline.
pub fn is_present() -> bool {
    auth_path().is_ok_and(|path| path.is_file())
}

fn read_credentials() -> Result<Credentials, String> {
    let text = std::fs::read_to_string(auth_path()?).map_err(|_| SIGN_IN.to_string())?;
    parse_credentials(&text)
}

pub fn error(message: String, scope: Option<&str>) -> UsageSnapshot {
    let mut snapshot = UsageSnapshot::error(message);
    snapshot.provider = Provider::Codex;
    snapshot.scope = scope.unwrap_or("codex:unknown").into();
    snapshot
}

fn duration_label(seconds: i64) -> String {
    if seconds % 86400 == 0 {
        format!("{}d", seconds / 86400)
    } else if seconds % 3600 == 0 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}m", (seconds / 60).max(1))
    }
}

fn window(v: &Value, id: &str, name: Option<&str>) -> Option<LimitWindow> {
    let pct = v.get("used_percent")?.as_f64()?;
    let seconds = v.get("limit_window_seconds")?.as_i64()?;
    if !pct.is_finite() || !(0.0..=100.0).contains(&pct) || !(1..=366 * 86400).contains(&seconds) {
        return None;
    }
    let duration = duration_label(seconds);
    Some(LimitWindow {
        id: id.into(),
        label: name.map(|n| format!("{n} {duration}")).unwrap_or(duration),
        utilization: pct,
        reset_at: v
            .get("reset_at")
            .and_then(Value::as_i64)
            .filter(|s| *s > 0 && *s <= i64::MAX / 1000),
        stale: false,
        window_seconds: Some(seconds),
    })
}

fn append_windows(windows: &mut Vec<LimitWindow>, v: &Value, ids: [&str; 2], name: Option<&str>) {
    for (key, id) in ["primary_window", "secondary_window"].into_iter().zip(ids) {
        if let Some(w) = window(&v[key], id, name) {
            if !windows.iter().any(|existing| existing.id == w.id) {
                windows.push(w);
            }
        }
    }
}

fn parse_usage(v: &Value, scope: &str) -> Result<UsageSnapshot, String> {
    let mut windows = Vec::new();
    append_windows(
        &mut windows,
        &v["rate_limit"],
        [ID_SESSION, ID_WEEKLY_ALL],
        None,
    );
    if let Some(extra) = v["additional_rate_limits"].as_array() {
        for limit in extra {
            let Some(key) = nonempty(limit.get("metered_feature"))
                .or_else(|| nonempty(limit.get("limit_name")))
            else {
                continue;
            };
            let name = nonempty(limit.get("limit_name")).unwrap_or(key);
            // Encode the identifier to avoid collisions between punctuation variants.
            let key = URL_SAFE_NO_PAD.encode(key);
            append_windows(
                &mut windows,
                &limit["rate_limit"],
                [&format!("session:{key}"), &format!("weekly_scoped:{key}")],
                Some(name),
            );
        }
    }
    append_windows(
        &mut windows,
        &v["code_review_rate_limit"],
        ["session:code-review", "weekly_scoped:code-review"],
        Some("Review"),
    );
    if windows.is_empty() {
        return Err("Codex returned no supported usage windows for this account".into());
    }
    let mut snapshot = UsageSnapshot::ok("codex-oauth", windows);
    snapshot.provider = Provider::Codex;
    snapshot.scope = scope.into();
    Ok(snapshot)
}

fn request(client: &reqwest::Client, credentials: &Credentials) -> reqwest::RequestBuilder {
    client
        .get(USAGE_URL)
        .timeout(Duration::from_secs(30))
        .bearer_auth(&credentials.token)
        .header("ChatGPT-Account-Id", &credentials.account)
        .header("Accept", "application/json")
}

pub struct PollResult {
    pub snapshot: UsageSnapshot,
    pub transient: bool,
    pub rate_limited: bool,
}

impl PollResult {
    fn failed(message: String, scope: Option<&str>) -> Self {
        Self {
            snapshot: error(message, scope),
            transient: false,
            rate_limited: false,
        }
    }
}

/// Read credentials even during a cooldown, so a different account can poll immediately.
/// Credentials are never refreshed or written here; error bodies stay out of the UI/logs.
pub async fn poll(client: &reqwest::Client, cooldown: Option<(&str, i64)>) -> Option<PollResult> {
    let credentials = match read_credentials() {
        Ok(c) => c,
        Err(e) => return Some(PollResult::failed(e, None)),
    };
    if cooldown_active(cooldown, &credentials.scope, crate::usage::now_ms()) {
        return None;
    }
    let result = fetch_with_retry(
        request(client, &credentials),
        &credentials.scope,
        Duration::from_secs(2),
    )
    .await;
    // Don't publish a response for an account that was replaced during the request.
    match read_credentials() {
        Ok(current) if current.scope != credentials.scope => {
            return Some(PollResult::failed(
                "Codex account changed — refreshing".into(),
                Some(&current.scope),
            ));
        }
        Err(e) => return Some(PollResult::failed(e, None)),
        _ => {}
    }
    Some(match result {
        Ok(snapshot) => PollResult {
            snapshot,
            transient: false,
            rate_limited: false,
        },
        Err(failure) => {
            let mut snapshot = error(failure.message, Some(&credentials.scope));
            snapshot.retry_at = failure.retry_at;
            PollResult {
                snapshot,
                transient: failure.transient,
                rate_limited: failure.rate_limited,
            }
        }
    })
}

fn cooldown_active(cooldown: Option<(&str, i64)>, scope: &str, now: i64) -> bool {
    cooldown.is_some_and(|(account, deadline)| account == scope && now < deadline)
}

#[derive(Debug)]
struct FetchFailure {
    message: String,
    transient: bool,
    rate_limited: bool,
    retry_at: Option<i64>,
}

impl FetchFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            transient: false,
            rate_limited: false,
            retry_at: None,
        }
    }

    fn transport(error: reqwest::Error) -> Self {
        let mut failure = Self::new(if error.is_timeout() {
            "Codex usage request timed out — retrying"
        } else {
            "Cannot reach Codex usage — check your connection"
        });
        failure.transient =
            error.is_timeout() || error.is_connect() || error.is_body() || error.is_request();
        failure
    }
}

fn retry_after(value: &str, now: i64) -> Option<i64> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        let seconds: u64 = value.parse().ok()?;
        return Some(
            now.saturating_add(i64::try_from(seconds.saturating_mul(1000)).unwrap_or(i64::MAX)),
        );
    }
    let date = httpdate::parse_http_date(value).ok()?;
    let millis = i64::try_from(date.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()?;
    Some(millis.max(now))
}

async fn fetch_with_retry(
    builder: reqwest::RequestBuilder,
    scope: &str,
    delay: Duration,
) -> Result<UsageSnapshot, FetchFailure> {
    let retry = builder.try_clone();
    let result = fetch_usage(builder, scope).await;
    // Never retry auth, malformed data or 429 here. A server-requested pause
    // belongs to the poll scheduler instead of keeping a request task asleep.
    if result
        .as_ref()
        .is_err_and(|e| e.transient && !e.rate_limited && e.retry_at.is_none())
    {
        if let Some(retry) = retry {
            tokio::time::sleep(delay).await;
            return fetch_usage(retry, scope).await;
        }
    }
    result
}

async fn fetch_usage(
    builder: reqwest::RequestBuilder,
    scope: &str,
) -> Result<UsageSnapshot, FetchFailure> {
    let response = builder.send().await.map_err(FetchFailure::transport)?;
    let status = response.status().as_u16();
    if (200..=299).contains(&status) {
        let body = response.json::<Value>().await.map_err(|e| {
            if e.is_timeout() || e.is_body() {
                FetchFailure::transport(e)
            } else {
                FetchFailure::new("Codex usage returned an unreadable response")
            }
        })?;
        return parse_usage(&body, scope).map_err(FetchFailure::new);
    }
    let mut failure = FetchFailure::new(match status {
        401 => "Codex login expired — open Codex to refresh your login, then retry".into(),
        403 => "Codex usage access denied for this account".into(),
        429 => "Codex usage rate-limited — waiting before retrying".into(),
        status => format!("Codex usage: HTTP {status}"),
    });
    failure.rate_limited = status == 429;
    failure.transient = matches!(status, 408 | 500 | 502 | 503 | 504);
    if failure.rate_limited || failure.transient {
        failure.retry_at = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| retry_after(v, crate::usage::now_ms()));
    }
    Err(failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn http_failures_are_classified_without_exposing_response_bodies() {
        use std::io::{Read, Write};
        for (status, expected, limited) in [
            (401, "Codex login expired", false),
            (403, "Codex usage access denied", false),
            (429, "Codex usage rate-limited", true),
            (500, "Codex usage: HTTP 500", false),
            (200, "Codex usage returned an unreadable response", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/usage", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut input = [0; 4096];
                let _ = socket.read(&mut input).unwrap();
                let body = "private-response-secret";
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let result =
                tauri::async_runtime::block_on(fetch_usage(client.get(url), "codex:fixture"));
            let failure = result.unwrap_err();
            let message = failure.message;
            assert!(message.starts_with(expected), "{message}");
            assert!(!message.contains("private-response-secret"));
            assert_eq!(failure.rate_limited, limited);
            assert_eq!(failure.transient, status == 500);
            server.join().unwrap();
        }
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        let now = 1_800_000_000_000;
        assert_eq!(retry_after("120", now), Some(now + 120_000));
        assert_eq!(retry_after("0", now), Some(now));
        let date =
            httpdate::fmt_http_date(UNIX_EPOCH + Duration::from_millis((now + 90_000) as u64));
        assert_eq!(retry_after(&date, now), Some(now + 90_000));
        assert_eq!(retry_after(&date, now + 120_000), Some(now + 120_000));
        assert_eq!(retry_after("18446744073709551615", now), Some(i64::MAX));
        for invalid in ["", "invalid", "-5", "+5", "1.5"] {
            assert_eq!(retry_after(invalid, now), None);
        }
    }

    #[test]
    fn cooldown_is_bound_to_account_and_deadline() {
        let cooldown = Some(("codex:a", 1000));
        assert!(cooldown_active(cooldown, "codex:a", 999));
        assert!(!cooldown_active(cooldown, "codex:a", 1000));
        assert!(!cooldown_active(cooldown, "codex:b", 999));
        assert!(!cooldown_active(None, "codex:a", 999));
    }

    // All retry checks use loopback responses and fake credentials, never OpenAI.
    fn retry_fixture(responses: Vec<(u16, &str, &str)>) -> Result<UsageSnapshot, FetchFailure> {
        use std::io::{Read, Write};
        let responses: Vec<_> = responses
            .into_iter()
            .map(|(status, headers, body)| (status, headers.to_string(), body.to_string()))
            .collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            for (status, headers, body) in responses {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "missing expected retry"
                            );
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(e) => panic!("fixture accept: {e}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let _ = socket.read(&mut [0; 4096]).unwrap();
                write!(socket, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
            }
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let result = tauri::async_runtime::block_on(fetch_with_retry(
            client.get(url),
            "codex:fixture",
            Duration::from_millis(1),
        ));
        server.join().unwrap();
        result
    }

    #[test]
    fn transient_server_failure_retries_once_and_can_recover() {
        let valid =
            r#"{"rate_limit":{"primary_window":{"used_percent":21,"limit_window_seconds":18000}}}"#;
        let recovered = retry_fixture(vec![(503, "", "secret"), (200, "", valid)]).unwrap();
        assert_eq!(recovered.windows[0].utilization, 21.0);
        let failed = retry_fixture(vec![(502, "", "secret"), (502, "", "secret")]).unwrap_err();
        assert_eq!(failed.message, "Codex usage: HTTP 502");
        assert!(failed.transient);
    }

    #[test]
    fn auth_invalid_data_and_server_cooldowns_never_get_an_immediate_retry() {
        for (status, headers, message) in [
            (401, "", "Codex login expired"),
            (403, "", "Codex usage access denied"),
            (200, "", "Codex usage returned an unreadable response"),
            (429, "", "Codex usage rate-limited"),
            (429, "Retry-After: 120\r\n", "Codex usage rate-limited"),
            (503, "Retry-After: 120\r\n", "Codex usage: HTTP 503"),
        ] {
            let start = crate::usage::now_ms();
            let failure = retry_fixture(vec![(status, headers, "secret")]).unwrap_err();
            assert!(failure.message.starts_with(message), "{}", failure.message);
            assert_eq!(failure.rate_limited, status == 429);
            if !headers.is_empty() {
                assert!(failure.retry_at.unwrap() >= start + 120_000);
            }
        }
    }

    #[test]
    fn transport_timeouts_retry_once_and_remain_distinct_from_http_errors() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut sockets = Vec::new();
            while sockets.len() < 2 {
                match listener.accept() {
                    Ok((socket, _)) => sockets.push(socket),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "timeout was not retried"
                        );
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            }
            // Keep both connections open without replying until the client times out.
            std::thread::sleep(Duration::from_millis(100));
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(30))
            .build()
            .unwrap();
        let failure = tauri::async_runtime::block_on(fetch_with_retry(
            client.get(url).bearer_auth("private-token"),
            "codex:fixture",
            Duration::from_millis(1),
        ))
        .unwrap_err();
        server.join().unwrap();
        assert_eq!(failure.message, "Codex usage request timed out — retrying");
        assert!(failure.transient);
        assert!(!failure.rate_limited);
        assert!(failure.retry_at.is_none());
    }

    #[test]
    fn codex_timeout_overrides_the_shared_clients_shorter_timeout() {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        let credentials = parse_credentials(&auth("account", "subject", "fake-token")).unwrap();
        let built = request(&client, &credentials).build().unwrap();
        assert_eq!(built.timeout(), Some(&Duration::from_secs(30)));
    }

    fn auth(account: &str, subject: &str, token: &str) -> String {
        let jwt = format!(
            "e30.{}.signature",
            URL_SAFE_NO_PAD.encode(json!({"sub":subject}).to_string())
        );
        json!({"tokens":{"access_token":token,"id_token":jwt,"account_id":account}}).to_string()
    }

    #[test]
    fn history_identity_survives_rotation_and_separates_users_and_workspaces() {
        let a = parse_credentials(&auth("workspace", "user", "old")).unwrap();
        let b = parse_credentials(&auth("workspace", "user", "new")).unwrap();
        assert_eq!(a.scope, b.scope);
        assert_ne!(
            a.scope,
            parse_credentials(&auth("workspace", "other", "new"))
                .unwrap()
                .scope
        );
        assert_ne!(
            a.scope,
            parse_credentials(&auth("other", "user", "new"))
                .unwrap()
                .scope
        );
        assert!(!a.scope.contains("workspace"));
        let r = request(&reqwest::Client::new(), &a).build().unwrap();
        assert_eq!(r.url().as_str(), USAGE_URL);
        assert_eq!(r.headers()["authorization"], "Bearer old");
        assert_eq!(r.headers()["chatgpt-account-id"], "workspace");
    }

    #[test]
    fn missing_malformed_and_api_key_auth_fail_without_echoing_credentials() {
        for text in [
            "invalid",
            "{}",
            r#"{"OPENAI_API_KEY":"secret"}"#,
            r#"{"auth_mode":"apikey","tokens":{"access_token":"secret"}}"#,
        ] {
            let err = parse_credentials(text).err().unwrap();
            assert!(!err.contains("secret"));
        }
    }

    #[test]
    fn parses_partial_and_additional_limits_using_reported_durations() {
        let w = json!({"used_percent":21,"limit_window_seconds":18000,"reset_at":1789170838_i64});
        let s = parse_usage(&json!({"rate_limit":{"primary_window":w},
            "additional_rate_limits":[null, {"metered_feature":"spark","limit_name":"Spark","rate_limit":{
                "secondary_window":{"used_percent":46,"limit_window_seconds":604800,"reset_at":1789501467_i64}}}]}), "codex:test").unwrap();
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.windows[0].label, "5h");
        assert_eq!(s.windows[0].window_seconds, Some(18000));
        assert_eq!(s.windows[1].label, "Spark 7d");
        assert_eq!(s.windows[1].utilization, 46.0);
        assert_eq!(s.provider, Provider::Codex);
        assert_eq!(s.scope, "codex:test");
    }

    #[test]
    fn missing_or_invalid_limits_are_not_zero_usage() {
        for v in [
            json!({}),
            json!({"rate_limit":null}),
            json!({"rate_limit":{"primary_window":{
            "used_percent":101,"limit_window_seconds":18000}}}),
        ] {
            assert!(parse_usage(&v, "codex:test").is_err());
        }
    }
}
