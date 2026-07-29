// Session-keeper status line for the settings window. The keeper itself lives
// in the backend (keeper.rs); this only puts its last action into words.

/** Elapsed time in the same shape as the widget's reset countdowns. */
function formatAgo(ms: number): string {
  const mins = Math.floor(ms / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  if (mins < 1440) return `${Math.floor(mins / 60)}h ${mins % 60}m ago`;
  return `${Math.floor(mins / 1440)}d ${Math.floor((mins % 1440) / 60)}h ago`;
}

/**
 * The keeper's status line, or "" when there is nothing to say (it's off, so
 * the line would only be noise under a checkbox that already says as much).
 *
 * `lastKeepaliveAt` is when the keeper last *ran*, not when it last succeeded
 * (the backend stamps it before the request so a failure still starts the
 * cooldown), so the wording claims nothing about the outcome.
 *
 * `lastKeepaliveAt` and `now` are epoch ms.
 */
export function keeperStatus(
  enabled: boolean,
  lastKeepaliveAt: number | null,
  now: number = Date.now(),
): string {
  if (!enabled) return "";
  if (lastKeepaliveAt === null) return "Not run yet.";
  // A timestamp ahead of us means the clock moved, not that the future
  // happened; "just now" is the honest reading of it.
  return `Last run ${formatAgo(Math.max(0, now - lastKeepaliveAt))}.`;
}
