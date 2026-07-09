use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::usage::{self, UsageSnapshot};

/// Wakes the poll loop early for an immediate refresh (tray "Refresh now" / UI button).
pub struct RefreshSignal(pub Arc<Notify>);

const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// A single failed poll usually self-heals by the next tick, so while good
/// data is on screen the error state is held back until this many polls in a
/// row have failed. With nothing good to show, an error surfaces immediately.
const ERROR_GRACE_POLLS: u32 = 3;
/// The fallback probe consumes quota, so even when enabled it never fires
/// more than once per this interval while the usage endpoint stays down.
const PROBE_MIN_INTERVAL_MS: i64 = 5 * 60_000;
/// Successive delays before retrying the usage endpoint after an HTTP 429 —
/// its per-account bucket stays exhausted if it keeps being hit every tick.
/// Other failures (network blips) retry at the normal poll interval.
const BACKOFF_STEPS_MS: [i64; 4] = [2 * 60_000, 4 * 60_000, 8 * 60_000, 15 * 60_000];
const OAUTH_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_BETA: &str = "oauth-2025-04-20";

/// Delay before the `n`th consecutive rate-limited attempt is retried, plus
/// up to 30s of clock-derived jitter so the app doesn't stay synchronized
/// with other pollers of the same account (Claude Code polls this too).
fn backoff_ms(consecutive_429s: u32, now_ms: i64) -> i64 {
    let idx = (consecutive_429s.saturating_sub(1) as usize).min(BACKOFF_STEPS_MS.len() - 1);
    BACKOFF_STEPS_MS[idx] + now_ms % 30_000
}

pub fn spawn(app: AppHandle) {
    let notify = app.state::<RefreshSignal>().0.clone();
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("tokometer/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build http client");
        let mut last_probe_ms: i64 = 0;
        let mut consecutive_failures: u32 = 0;
        let mut rate_limited_polls: u32 = 0;
        // Next moment the usage endpoint may be tried; pushed into the future
        // while it is rate-limiting us. The probe keeps supplying data in the
        // meantime, and a manual refresh clears the backoff.
        let mut next_oauth_ms: i64 = 0;
        loop {
            let now = usage::now_ms();
            let try_oauth = now >= next_oauth_ms;
            let probe_allowed = {
                let state = app.state::<crate::state::AppState>();
                let enabled = state.0.lock().unwrap().probe_fallback;
                enabled && now - last_probe_ms >= PROBE_MIN_INTERVAL_MS
            };
            let outcome = poll_once(&client, try_oauth, probe_allowed).await;
            if outcome.probed {
                last_probe_ms = usage::now_ms();
            }
            if try_oauth {
                if outcome.oauth_rate_limited {
                    rate_limited_polls += 1;
                    let now = usage::now_ms();
                    next_oauth_ms = now + backoff_ms(rate_limited_polls, now);
                } else {
                    rate_limited_polls = 0;
                    next_oauth_ms = 0;
                }
            }
            let Some(snapshot) = outcome.snapshot else {
                // Nothing was due this tick (usage endpoint backing off,
                // probe not due) — sleep without touching the shown state.
                if wait_or_refresh(&notify).await {
                    next_oauth_ms = 0;
                }
                continue;
            };
            if snapshot.status == "ok" {
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
                let showing_good_data = {
                    let state = app.state::<crate::state::AppState>();
                    let s = state.0.lock().unwrap();
                    s.last_usage.as_ref().is_some_and(|u| u.status == "ok")
                };
                if showing_good_data && consecutive_failures < ERROR_GRACE_POLLS {
                    if wait_or_refresh(&notify).await {
                        next_oauth_ms = 0;
                    }
                    continue;
                }
            }
            {
                let state = app.state::<crate::state::AppState>();
                state.0.lock().unwrap().last_usage = Some(snapshot.clone());
            }
            crate::state::save(&app);
            let recorded = {
                let log = app.state::<crate::history::HistoryLog>();
                let mut samples = log.0.lock().unwrap();
                crate::history::record(&mut samples, &snapshot, usage::now_ms())
            };
            if recorded {
                crate::history::save(&app);
            }
            let _ = app.emit("usage://update", &snapshot);
            crate::tray::update(&app, &snapshot);

            if wait_or_refresh(&notify).await {
                next_oauth_ms = 0;
            }
        }
    });
}

/// Sleep one poll interval; returns true when woken early by a manual
/// refresh, which should retry the usage endpoint immediately.
async fn wait_or_refresh(notify: &Notify) -> bool {
    tokio::time::timeout(POLL_INTERVAL, notify.notified())
        .await
        .is_ok()
}

enum OauthError {
    RateLimited(String),
    Other(String),
}

struct PollOutcome {
    /// `None` when neither request was due this tick.
    snapshot: Option<UsageSnapshot>,
    /// Whether the (quota-consuming) messages probe was attempted,
    /// so the loop can rate-limit it.
    probed: bool,
    /// Whether the usage endpoint answered HTTP 429, so the loop can back off.
    oauth_rate_limited: bool,
}

impl PollOutcome {
    fn done(snapshot: UsageSnapshot, probed: bool, oauth_rate_limited: bool) -> Self {
        Self {
            snapshot: Some(snapshot),
            probed,
            oauth_rate_limited,
        }
    }
}

async fn poll_once(client: &reqwest::Client, try_oauth: bool, probe_allowed: bool) -> PollOutcome {
    let creds = match crate::credentials::read() {
        Ok(c) => c,
        Err(e) => return PollOutcome::done(UsageSnapshot::error(e), false, false),
    };
    let mut rate_limited = false;
    let oauth_err = if try_oauth {
        match fetch_oauth(client, &creds.token).await {
            Ok(snapshot) => return PollOutcome::done(snapshot, false, false),
            Err(OauthError::RateLimited(msg)) => {
                rate_limited = true;
                msg
            }
            Err(OauthError::Other(msg)) => msg,
        }
    } else {
        "oauth usage: backing off after HTTP 429".into()
    };
    if probe_allowed {
        let snapshot = match fetch_messages(client, &creds.token).await {
            Ok(snapshot) => snapshot,
            // Only blame the token once the request has actually failed —
            // Claude Code refreshes it in the background, so a stale
            // `expiresAt` usually resolves itself by the next poll.
            Err(_) if creds.looks_expired() => UsageSnapshot::error(
                "token expired — open Claude Code to refresh it, or run `claude login`".into(),
            ),
            Err(probe_err) => UsageSnapshot::error(format!("{oauth_err}; fallback: {probe_err}")),
        };
        return PollOutcome::done(snapshot, true, rate_limited);
    }
    if !try_oauth {
        return PollOutcome {
            snapshot: None,
            probed: false,
            oauth_rate_limited: false,
        };
    }
    // A 429 says nothing about the token, so don't surface expiry over it.
    let snapshot = if creds.looks_expired() && !rate_limited {
        UsageSnapshot::error(
            "token expired — open Claude Code to refresh it, or run `claude login`".into(),
        )
    } else {
        UsageSnapshot::error(oauth_err)
    };
    PollOutcome::done(snapshot, false, rate_limited)
}

/// Primary: the OAuth usage endpoint — free, no tokens consumed.
async fn fetch_oauth(client: &reqwest::Client, token: &str) -> Result<UsageSnapshot, OauthError> {
    let resp = client
        .get(OAUTH_USAGE_URL)
        .bearer_auth(token)
        .header("anthropic-beta", ANTHROPIC_BETA)
        .send()
        .await
        .map_err(|e| OauthError::Other(format!("oauth usage: {e}")))?;
    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(OauthError::RateLimited(format!(
            "oauth usage: HTTP {status}"
        )));
    }
    if !status.is_success() {
        return Err(OauthError::Other(format!("oauth usage: HTTP {status}")));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| OauthError::Other(format!("oauth usage body: {e}")))?;
    usage::from_oauth_body(&body)
        .ok_or_else(|| OauthError::Other("oauth usage: unexpected body shape".into()))
}

/// Fallback: minimal 1-token probe, reading the rate-limit response headers
/// (exactly what Clawdmeter's daemon does). Only used when the usage endpoint
/// fails, and can be disabled — it spends a sliver of the quota it measures.
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
    usage::from_ratelimit_headers(resp.headers()).ok_or_else(|| {
        format!(
            "messages probe: no ratelimit headers (HTTP {})",
            resp.status()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_caps() {
        // Jitter is now_ms % 30_000; a multiple of 30s makes it zero.
        let now = 30_000;
        assert_eq!(backoff_ms(1, now), 2 * 60_000);
        assert_eq!(backoff_ms(2, now), 4 * 60_000);
        assert_eq!(backoff_ms(3, now), 8 * 60_000);
        assert_eq!(backoff_ms(4, now), 15 * 60_000);
        assert_eq!(backoff_ms(99, now), 15 * 60_000);
    }

    #[test]
    fn backoff_jitter_stays_under_thirty_seconds() {
        for now in [0, 1, 12_345, 29_999, 61_234] {
            let d = backoff_ms(1, now);
            assert!((2 * 60_000..2 * 60_000 + 30_000).contains(&d), "delay {d}");
        }
    }

    #[test]
    fn backoff_handles_zero_count() {
        // Defensive: a zero count (never happens in the loop) uses the first step.
        assert_eq!(backoff_ms(0, 0), 2 * 60_000);
    }
}
