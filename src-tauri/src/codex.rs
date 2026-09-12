//! Read-only Codex OAuth usage. No CLI, app-server, token refresh or auth-file writes.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

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
        .bearer_auth(&credentials.token)
        .header("ChatGPT-Account-Id", &credentials.account)
        .header("Accept", "application/json")
}

/// Return the snapshot and whether to back off. Error bodies never reach logs or the UI.
pub async fn poll(client: &reqwest::Client) -> (UsageSnapshot, bool) {
    let credentials = match read_credentials() {
        Ok(c) => c,
        Err(e) => return (error(e, None), false),
    };
    let (result, rate_limited) =
        fetch_usage(request(client, &credentials), &credentials.scope).await;
    // Don't publish a response for an account that was replaced during the request.
    match read_credentials() {
        Ok(current) if current.scope != credentials.scope => {
            return (
                error(
                    "Codex account changed — refreshing".into(),
                    Some(&current.scope),
                ),
                false,
            )
        }
        Err(e) => return (error(e, None), false),
        _ => {}
    }
    (
        result.unwrap_or_else(|e| error(e, Some(&credentials.scope))),
        rate_limited,
    )
}

async fn fetch_usage(
    builder: reqwest::RequestBuilder,
    scope: &str,
) -> (Result<UsageSnapshot, String>, bool) {
    let mut rate_limited = false;
    let result = match builder.send().await {
        Ok(response) => match response.status().as_u16() {
            200..=299 => match response.json::<Value>().await {
                Ok(body) => parse_usage(&body, scope),
                Err(_) => Err("Codex usage returned an unreadable response".into()),
            },
            401 => Err("Codex login expired — open Codex to refresh your login, then retry".into()),
            403 => Err("Codex usage access denied for this account".into()),
            429 => {
                rate_limited = true;
                Err("Codex usage rate-limited — retrying after a short pause".into())
            }
            status => Err(format!("Codex usage: HTTP {status}")),
        },
        Err(_) => Err("Cannot reach Codex usage — check your connection".into()),
    };
    (result, rate_limited)
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
            let (result, backoff) =
                tauri::async_runtime::block_on(fetch_usage(client.get(url), "codex:fixture"));
            let message = result.unwrap_err();
            assert!(message.starts_with(expected), "{message}");
            assert!(!message.contains("private-response-secret"));
            assert_eq!(backoff, limited);
            server.join().unwrap();
        }
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
