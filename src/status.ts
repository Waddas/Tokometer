import type { UsageSnapshot } from "./api";

/** Terse widget guidance; the original safe error remains available in Settings. */
export function friendlyError(err: string): string {
  if (err.startsWith("Loading")) return err;
  if (err.startsWith("Codex login expired")) return "Open Codex to refresh your login";
  if (err.startsWith("Codex login unavailable")) return "Sign in to Codex to start tracking";
  if (err.startsWith("Codex requires") || err.startsWith("Codex account identity")) return "Sign in to Codex with ChatGPT";
  if (err.startsWith("Codex usage access denied")) return "Codex usage access denied";
  if (err.startsWith("Codex returned no")) return "Codex usage limits unavailable";
  if (err.startsWith("Codex account changed")) return "Codex account changed — refreshing";
  if (err.startsWith("Codex usage paused")) return "Codex usage paused";
  if (err.startsWith("Codex usage rate-limited")) return "Codex rate limit — waiting";
  if (err.startsWith("Codex usage request timed out")) return "Codex request timed out — retrying";
  if (err.startsWith("Codex usage returned an unreadable")) return "Codex sent an invalid response";
  const http = /^Codex usage: HTTP (\d{3})$/.exec(err);
  if (http) return Number(http[1]) >= 500
    ? `Codex service error (${http[1]}) — retrying`
    : `Codex usage error (HTTP ${http[1]})`;
  if (err.includes("no Claude credentials")) return "Sign in to Claude Code to start tracking";
  if (err.startsWith("token expired")) return "Token expired — open Claude Code";
  return "Can't reach usage API — retrying";
}

function age(ms: number): string {
  const minutes = Math.max(0, Math.floor(ms / 60_000));
  if (minutes < 1) return "<1m";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

export function usageStatus(snapshot: UsageSnapshot, now = Date.now()): string {
  if (snapshot.status === "ok") return "";
  const recent = snapshot.lastSuccessAt != null && now - snapshot.lastSuccessAt < 5 * 60_000;
  const message = snapshot.recovering && recent ? "Updating…" : friendlyError(snapshot.error ?? "");
  const parts = [message];
  if (snapshot.retryAt != null && snapshot.retryAt > now) {
    parts.push(`retry in ${Math.ceil((snapshot.retryAt - now) / 60_000)}m`);
  }
  if (snapshot.lastSuccessAt != null) parts.unshift(`${age(now - snapshot.lastSuccessAt)} old`);
  return parts.join(" · ");
}
