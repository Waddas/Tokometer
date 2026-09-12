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
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("tokometer/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build http client");
        let mut last_probe_ms: i64 = 0;
        let mut consecutive_failures: u32 = 0;
        let mut rate_limited_polls: u32 = 0;
        let mut next_oauth_ms: i64 = 0;
        let mut last_revision = u64::MAX;
        // Separate from Claude's schedule: manual refresh and provider switching
        // must not bypass a Codex server cooldown for the same account.
        let mut codex_cooldown: Option<(String, i64, u32)> = None;
        loop {
            // Provider visibility reflects local use, independent of API health.
            publish_availability(
                &app,
                crate::state::Provider::Claude,
                crate::credentials::is_present(),
            );
            publish_availability(
                &app,
                crate::state::Provider::Codex,
                crate::codex::is_present(),
            );
            let (provider, revision, probe_enabled) = {
                let state = app.state::<crate::state::AppState>();
                let s = state.0.lock().unwrap();
                (s.provider, s.provider_revision, s.probe_fallback)
            };
            if revision != last_revision {
                last_revision = revision;
                consecutive_failures = 0;
                rate_limited_polls = 0;
                next_oauth_ms = 0;
            }
            let now = usage::now_ms();
            let try_oauth = now >= next_oauth_ms;
            let outcome = match provider {
                crate::state::Provider::Claude => {
                    let probe_allowed =
                        probe_enabled && now - last_probe_ms >= PROBE_MIN_INTERVAL_MS;
                    poll_once(&client, try_oauth, probe_allowed).await
                }
                crate::state::Provider::Codex => {
                    let cooldown = codex_cooldown
                        .as_ref()
                        .map(|(scope, at, _)| (scope.as_str(), *at));
                    match crate::codex::poll(&client, cooldown).await {
                        Some(mut result) => {
                            let count = codex_cooldown
                                .as_ref()
                                .filter(|(scope, _, _)| scope == &result.snapshot.scope)
                                .map_or(0, |(_, _, count)| *count);
                            if result.rate_limited {
                                let count = count.saturating_add(1);
                                let now = usage::now_ms();
                                let deadline =
                                    codex_retry_deadline(count, now, result.snapshot.retry_at);
                                result.snapshot.retry_at = Some(deadline);
                                codex_cooldown =
                                    Some((result.snapshot.scope.clone(), deadline, count));
                            } else if let Some(deadline) = result.snapshot.retry_at {
                                codex_cooldown = Some((result.snapshot.scope.clone(), deadline, 0));
                            } else if codex_cooldown
                                .as_ref()
                                .is_some_and(|(scope, _, _)| scope == &result.snapshot.scope)
                            {
                                codex_cooldown = None;
                            }
                            let mut outcome = PollOutcome::done(result.snapshot, false, false);
                            outcome.transient = result.transient;
                            outcome
                        }
                        None => {
                            // Switching back clears last_usage. Show the pause instead of
                            // leaving the widget stuck on "Loading" until the next request.
                            let snapshot =
                                codex_cooldown.as_ref().and_then(|(scope, deadline, _)| {
                                    let state = app.state::<crate::state::AppState>();
                                    let s = state.0.lock().unwrap();
                                    if s.last_usage
                                        .as_ref()
                                        .is_some_and(|last| &last.scope == scope)
                                    {
                                        return None;
                                    }
                                    let mut paused = crate::codex::error(
                                        "Codex usage paused — waiting before retrying".into(),
                                        Some(scope),
                                    );
                                    paused.retry_at = Some(*deadline);
                                    Some(paused)
                                });
                            PollOutcome {
                                snapshot,
                                probed: false,
                                oauth_rate_limited: false,
                                transient: false,
                            }
                        }
                    }
                }
            };
            // A provider change invalidates the result; keep any account-specific Codex cooldown.
            if !app
                .state::<crate::state::AppState>()
                .0
                .lock()
                .unwrap()
                .accepts_poll(provider, revision)
            {
                continue;
            }
            if outcome.probed {
                last_probe_ms = usage::now_ms();
            }
            if try_oauth && provider == crate::state::Provider::Claude {
                if outcome.oauth_rate_limited {
                    rate_limited_polls += 1;
                    next_oauth_ms =
                        usage::now_ms() + backoff_ms(rate_limited_polls, usage::now_ms());
                } else {
                    rate_limited_polls = 0;
                    next_oauth_ms = 0;
                }
            }
            if let Some(mut snapshot) = outcome.snapshot {
                let (published, recorded) = {
                    let state = app.state::<crate::state::AppState>();
                    let mut s = state.0.lock().unwrap();
                    if !s.accepts_poll(provider, revision) {
                        continue;
                    }
                    let same_account = s
                        .last_usage
                        .as_ref()
                        .filter(|p| p.scope == snapshot.scope && p.provider == snapshot.provider);
                    if same_account.is_none() {
                        consecutive_failures = 0;
                    }
                    if snapshot.status == "ok" {
                        consecutive_failures = 0;
                    } else {
                        consecutive_failures += 1;
                    }
                    let hold_error = snapshot.status != "ok"
                        && consecutive_failures < ERROR_GRACE_POLLS
                        && same_account.is_some_and(|p| p.status == "ok")
                        // Authentication errors should immediately give actionable guidance.
                        && provider == crate::state::Provider::Claude;
                    if hold_error {
                        (false, false)
                    } else {
                        if let Some(previous) = same_account {
                            retain_previous(&mut snapshot, previous);
                            snapshot.recovering =
                                codex_grace(&snapshot, outcome.transient, consecutive_failures);
                        }
                        s.last_usage = Some(snapshot.clone());
                        let log = app.state::<crate::history::HistoryLog>();
                        let recorded = crate::history::record(
                            &mut log.0.lock().unwrap(),
                            &snapshot,
                            usage::now_ms(),
                        );
                        let _ = app.emit("usage://update", &snapshot);
                        (true, recorded)
                    }
                };
                if published {
                    crate::state::save(&app);
                    if recorded {
                        crate::history::save(&app);
                    }
                    // Render the current state, in case a provider switch happened after publication.
                    crate::tray::refresh(&app);
                }
            }
            if wait_or_refresh(&notify).await {
                next_oauth_ms = 0;
            }
        }
    });
}

fn publish_availability(app: &AppHandle, provider: crate::state::Provider, available: bool) {
    let updated = {
        let state = app.state::<crate::state::AppState>();
        let mut s = state.0.lock().unwrap();
        s.available_providers
            .update(provider, available)
            .then(|| (s.layout, s.effective_scale()))
    };
    if let Some((layout, scale)) = updated {
        crate::commands::resize_main(app, layout, scale);
        crate::tray::emit_state(app);
    }
}

/// Stale values survive failures/restarts, but never cross account or provider boundaries.
fn retain_previous(snapshot: &mut UsageSnapshot, previous: &UsageSnapshot) {
    if snapshot.provider != previous.provider || snapshot.scope != previous.scope {
        return;
    }
    if snapshot.status == "ok" {
        usage::carry_missing_windows(snapshot, previous);
    } else {
        snapshot.last_success_at = if previous.status == "ok" {
            Some(previous.fetched_at)
        } else {
            previous.last_success_at
        };
        snapshot.windows = previous
            .windows
            .iter()
            .cloned()
            .map(|mut w| {
                w.stale = true;
                w
            })
            .collect();
    }
}

fn codex_retry_deadline(count: u32, now: i64, server: Option<i64>) -> i64 {
    now.saturating_add(backoff_ms(count, now))
        .max(server.unwrap_or(now))
}

fn codex_grace(snapshot: &UsageSnapshot, transient: bool, failures: u32) -> bool {
    snapshot.provider == crate::state::Provider::Codex
        && snapshot.status != "ok"
        && transient
        && snapshot.retry_at.is_none()
        && failures < ERROR_GRACE_POLLS
        && !snapshot.windows.is_empty()
        && snapshot
            .last_success_at
            .is_some_and(|at| usage::now_ms().saturating_sub(at) < 5 * 60_000)
}

/// Sleep one poll interval; returns true when woken early by a manual
/// refresh. Codex still honors its account cooldown when woken early.
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
    transient: bool,
}

impl PollOutcome {
    fn done(snapshot: UsageSnapshot, probed: bool, oauth_rate_limited: bool) -> Self {
        Self {
            snapshot: Some(snapshot),
            probed,
            oauth_rate_limited,
            transient: false,
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
            transient: false,
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
    fn failures_preserve_only_the_same_accounts_windows_and_never_record_them() {
        let mut previous = UsageSnapshot::ok(
            "codex-oauth",
            vec![usage::LimitWindow {
                id: usage::ID_SESSION.into(),
                label: "5h".into(),
                utilization: 21.0,
                reset_at: Some(1789170838),
                stale: false,
                window_seconds: Some(18000),
            }],
        );
        previous.provider = crate::state::Provider::Codex;
        previous.scope = "codex:a".into();
        let mut failed = crate::codex::error("expired".into(), Some("codex:a"));
        retain_previous(&mut failed, &previous);
        assert_eq!(failed.windows[0].utilization, 21.0);
        assert!(failed.windows[0].stale);
        assert!(!crate::history::record(
            &mut Vec::new(),
            &failed,
            usage::now_ms()
        ));
        let restored: UsageSnapshot =
            serde_json::from_str(&serde_json::to_string(&failed).unwrap()).unwrap();
        let mut again = crate::codex::error("expired".into(), Some("codex:a"));
        retain_previous(&mut again, &restored);
        assert_eq!(again.windows.len(), 1);
        assert_eq!(again.last_success_at, Some(previous.fetched_at));
        assert_eq!(restored.last_success_at, Some(previous.fetched_at));
        let mut other = crate::codex::error("expired".into(), Some("codex:b"));
        retain_previous(&mut other, &previous);
        assert!(other.windows.is_empty());
        assert!(other.last_success_at.is_none());
        let mut claude = UsageSnapshot::error("offline".into());
        retain_previous(&mut claude, &previous);
        assert!(claude.windows.is_empty());
    }

    #[test]
    fn codex_grace_requires_recent_same_account_data_and_only_transient_failures() {
        let mut snapshot = crate::codex::error("temporary".into(), Some("codex:a"));
        snapshot.last_success_at = Some(usage::now_ms());
        // No reading to display means errors must surface immediately.
        assert!(!codex_grace(&snapshot, true, 1));
        snapshot.windows.push(usage::LimitWindow {
            id: usage::ID_SESSION.into(),
            label: "5h".into(),
            utilization: 21.0,
            reset_at: None,
            stale: true,
            window_seconds: Some(18000),
        });
        assert!(codex_grace(&snapshot, true, 1));
        assert!(codex_grace(&snapshot, true, 2));
        assert!(!codex_grace(&snapshot, true, 3));
        assert!(!codex_grace(&snapshot, false, 1)); // auth / malformed data
        snapshot.retry_at = Some(usage::now_ms() + 60_000);
        assert!(!codex_grace(&snapshot, true, 1)); // server-requested pause
        snapshot.retry_at = None;
        snapshot.last_success_at = Some(usage::now_ms() - 5 * 60_000);
        assert!(!codex_grace(&snapshot, true, 1));
        snapshot.last_success_at = None; // old saved error has no known age
        assert!(!codex_grace(&snapshot, true, 1));
    }

    #[test]
    fn codex_backoff_never_shortens_a_server_requested_pause() {
        assert_eq!(codex_retry_deadline(1, 30_000, Some(900_000)), 900_000);
        assert_eq!(codex_retry_deadline(1, 30_000, Some(31_000)), 150_000);
        assert_eq!(codex_retry_deadline(1, 30_000, None), 150_000);
    }

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
