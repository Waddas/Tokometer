use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::usage::{self, UsageSnapshot};

/// Wakes the poll loop early for an immediate refresh (tray "Refresh now" / UI button).
pub struct RefreshSignal(pub Arc<Notify>);

const POLL_INTERVAL: Duration = Duration::from_secs(60);
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

pub fn spawn(app: AppHandle) {
    let notify = app.state::<RefreshSignal>().0.clone();
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("clordgauge/0.1.0")
            .build()
            .expect("failed to build http client");
        loop {
            let snapshot = poll_once(&client).await;
            {
                let state = app.state::<crate::state::AppState>();
                state.0.lock().unwrap().last_usage = Some(snapshot.clone());
            }
            crate::state::save(&app);
            let _ = app.emit("usage://update", &snapshot);
            crate::tray::update(&app, &snapshot);

            // Sleep until the next tick — or earlier on a manual refresh.
            let _ = tokio::time::timeout(POLL_INTERVAL, notify.notified()).await;
        }
    });
}

async fn poll_once(client: &reqwest::Client) -> UsageSnapshot {
    let creds = match crate::credentials::read() {
        Ok(c) => c,
        Err(e) => return UsageSnapshot::error(e),
    };
    match fetch_oauth(client, &creds.token).await {
        Ok(snapshot) => snapshot,
        Err(oauth_err) => match fetch_messages(client, &creds.token).await {
            Ok(snapshot) => snapshot,
            // Only blame the token once the request has actually failed —
            // Claude Code refreshes it in the background, so a stale
            // `expiresAt` usually resolves itself by the next poll.
            Err(_) if creds.looks_expired() => UsageSnapshot::error(
                "token expired — open Claude Code to refresh it, or run `claude login`".into(),
            ),
            Err(probe_err) => UsageSnapshot::error(format!("{oauth_err}; fallback: {probe_err}")),
        },
    }
}

/// Primary: the OAuth usage endpoint — free, no tokens consumed.
async fn fetch_oauth(client: &reqwest::Client, token: &str) -> Result<UsageSnapshot, String> {
    let resp = client
        .get(OAUTH_USAGE_URL)
        .bearer_auth(token)
        .header("anthropic-beta", ANTHROPIC_BETA)
        .send()
        .await
        .map_err(|e| format!("oauth usage: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("oauth usage: HTTP {status}"));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("oauth usage body: {e}"))?;
    usage::from_oauth_body(&body).ok_or_else(|| "oauth usage: unexpected body shape".into())
}

/// Fallback: minimal 1-token probe, reading the rate-limit response headers
/// (exactly what Clawdmeter's daemon does).
async fn fetch_messages(client: &reqwest::Client, token: &str) -> Result<UsageSnapshot, String> {
    let payload = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "hi"}],
    });
    let resp = client
        .post(MESSAGES_URL)
        .bearer_auth(token)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", ANTHROPIC_BETA)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("messages probe: {e}"))?;
    // Rate-limit headers are present even on 429 — parse regardless of status.
    usage::from_ratelimit_headers(resp.headers())
        .ok_or_else(|| format!("messages probe: no ratelimit headers (HTTP {})", resp.status()))
}
