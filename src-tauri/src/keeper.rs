//! Session keeper — keeps a 5-hour window always open.
//!
//! The 5-hour limit runs on a window that only *starts* when a request is
//! made, and expires five hours later. An idle account therefore has no window
//! running at all: after one resets, the clock stops until the next request.
//! Every hour spent in that gap is a slice of a window the plan would have
//! granted, so nights and weekends quietly cost windows.
//!
//! When enabled, the keeper sends the same minimal 1-token message the fallback
//! probe uses as soon as a window has rolled over, which opens the next one
//! immediately and keeps the windows rolling back-to-back.

use crate::usage::UsageSnapshot;

/// Wait this long past a window's reset before opening the next one. A message
/// sent before the old window has really closed lands *inside* it — spending
/// quota without starting anything — so this absorbs clock skew between us and
/// the API, and any lag in the reset time we were last told about.
pub const RESET_LAG_MS: i64 = 60_000;

/// Floor on the gap between keeper messages, whatever the snapshot says. If a
/// send doesn't visibly open a window (unexpected response shape, a reset time
/// that never advances), this is what stops the loop spending a request a
/// minute forever.
pub const MIN_INTERVAL_MS: i64 = 10 * 60_000;

/// Above this 7-day utilization the keeper stands down: the weekly limit, not
/// the 5-hour one, is what's binding, so another window buys nothing — and the
/// request would likely be refused anyway.
pub const WEEKLY_CEILING: f64 = 99.0;

/// Whether a keeper message should be sent right now.
///
/// `last_send_ms` is when the keeper last sent one (0 if never); both it and
/// `now_ms` are unix epoch milliseconds.
pub fn should_start_session(snapshot: &UsageSnapshot, last_send_ms: i64, now_ms: i64) -> bool {
    // Only act on data we trust. An error snapshot says nothing about whether
    // a window is open, and guessing spends real quota.
    if snapshot.status != "ok" {
        return false;
    }
    if now_ms - last_send_ms < MIN_INTERVAL_MS {
        return false;
    }
    if snapshot
        .seven_day
        .as_ref()
        .is_some_and(|w| w.utilization >= WEEKLY_CEILING)
    {
        return false;
    }
    let Some(five) = snapshot.five_hour.as_ref() else {
        return false;
    };
    match five.reset_at {
        // A reset time still in the future means a window is already running,
        // and nothing needs doing until it has been past for RESET_LAG_MS.
        Some(reset_at) => now_ms >= reset_at.saturating_mul(1000) + RESET_LAG_MS,
        // No reset time to go by: only an untouched window is safe to read as
        // "nothing running" — any utilization implies an open window whose
        // reset time we simply can't see.
        None => five.utilization <= 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::UsageWindow;

    const HOUR_MS: i64 = 3_600_000;
    /// A "now" far enough from zero that the cooldown never accidentally
    /// gates a test using the default `last_send_ms` of 0.
    const NOW: i64 = 1_780_596_000_000;

    fn window(utilization: f64, reset_at: Option<i64>) -> UsageWindow {
        UsageWindow {
            utilization,
            reset_at,
        }
    }

    /// A snapshot whose 5-hour window resets `offset_ms` from NOW (negative =
    /// already past), with a comfortable 20% weekly figure.
    fn snapshot_resetting(offset_ms: i64) -> UsageSnapshot {
        UsageSnapshot::ok(
            "oauth",
            window(50.0, Some((NOW + offset_ms) / 1000)),
            window(20.0, None),
            None,
        )
    }

    #[test]
    fn opens_a_window_once_the_previous_one_has_reset() {
        let snap = snapshot_resetting(-2 * RESET_LAG_MS);
        assert!(should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn waits_while_a_window_is_still_running() {
        let snap = snapshot_resetting(2 * HOUR_MS);
        assert!(!should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn holds_off_until_the_reset_lag_has_passed() {
        // Sending inside the old window would spend quota without opening a
        // new one, so the moment of reset itself is too early.
        let snap = snapshot_resetting(0);
        assert!(!should_start_session(&snap, 0, NOW));
        assert!(!should_start_session(&snap, 0, NOW + RESET_LAG_MS - 1));
        assert!(should_start_session(&snap, 0, NOW + RESET_LAG_MS));
    }

    #[test]
    fn respects_the_minimum_interval_between_sends() {
        let snap = snapshot_resetting(-2 * HOUR_MS);
        assert!(!should_start_session(&snap, NOW - MIN_INTERVAL_MS + 1, NOW));
        assert!(should_start_session(&snap, NOW - MIN_INTERVAL_MS, NOW));
    }

    #[test]
    fn never_sent_before_counts_as_long_ago() {
        let snap = snapshot_resetting(-2 * HOUR_MS);
        assert!(should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn stands_down_when_the_weekly_limit_is_effectively_spent() {
        let mut snap = snapshot_resetting(-2 * HOUR_MS);
        snap.seven_day = Some(window(WEEKLY_CEILING, None));
        assert!(!should_start_session(&snap, 0, NOW));

        snap.seven_day = Some(window(WEEKLY_CEILING - 0.5, None));
        assert!(should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn ignores_error_snapshots() {
        let snap = UsageSnapshot::error("oauth usage: HTTP 500".into());
        assert!(!should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn ignores_snapshots_without_a_five_hour_window() {
        let mut snap = snapshot_resetting(-2 * HOUR_MS);
        snap.five_hour = None;
        assert!(!should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn treats_an_unused_window_with_no_reset_time_as_idle() {
        let snap = UsageSnapshot::ok("messages", window(0.0, None), window(20.0, None), None);
        assert!(should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn assumes_a_window_is_running_when_usage_exists_but_no_reset_time_does() {
        let snap = UsageSnapshot::ok("messages", window(12.0, None), window(20.0, None), None);
        assert!(!should_start_session(&snap, 0, NOW));
    }

    #[test]
    fn a_full_five_hour_window_still_waits_for_its_reset() {
        // "Limited" is not a reason to send — the window is open, just spent.
        let mut snap = snapshot_resetting(HOUR_MS);
        snap.five_hour = Some(window(100.0, Some((NOW + HOUR_MS) / 1000)));
        snap.five_hour_status = Some("limited".into());
        assert!(!should_start_session(&snap, 0, NOW));
    }
}
