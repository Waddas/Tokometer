#!/bin/sh
# Prints the current Claude usage from Tokometer, for Claude Code sessions.
#
# Reads usage-status.json, which the Tokometer app writes after every poll
# (src-tauri/src/status_file.rs). Plain POSIX sh with sed, so it needs no jq,
# node or python; Claude Code runs it through bash, or Git Bash on Windows.

APP_ID=com.bradleywadsworth.tokometer
# Polls run every minute, but rate-limit backoff can pause them for 15 minutes.
MAX_AGE_S=1200

candidates() {
  if [ -n "$TOKOMETER_USAGE_FILE" ]; then
    echo "$TOKOMETER_USAGE_FILE"
    return
  fi
  appdata=
  if [ -n "$APPDATA" ]; then
    appdata=$APPDATA
    command -v cygpath >/dev/null 2>&1 && appdata=$(cygpath -u "$APPDATA")
  fi
  for id in "$APP_ID" "$APP_ID.dev"; do
    echo "$HOME/Library/Application Support/$id/usage-status.json"
    echo "${XDG_CONFIG_HOME:-$HOME/.config}/$id/usage-status.json"
    [ -n "$appdata" ] && echo "$appdata/$id/usage-status.json"
  done
}

# Top-level scalars sit on their own lines in the pretty-printed file.
str_field() { sed -n "s/^  \"$1\": \"\(.*\)\",\{0,1\}\$/\1/p" "$2"; }
num_field() { sed -n "s/^  \"$1\": \([0-9][0-9]*\),\{0,1\}\$/\1/p" "$2"; }

# The freshest usable Claude status file.
now=$(date +%s)
file=
best=
while IFS= read -r f; do
  [ -f "$f" ] || continue
  [ "$(num_field version "$f")" = 1 ] || continue
  [ "$(str_field provider "$f")" = claude ] || continue
  at=$(num_field observedAt "$f")
  [ -n "$at" ] && [ $((now - at)) -le "$MAX_AGE_S" ] || continue
  if [ -z "$best" ] || [ "$at" -gt "$best" ]; then
    best=$at
    file=$f
  fi
done <<EOF
$(candidates)
EOF

if [ -z "$file" ]; then
  echo "No current Claude usage from Tokometer. Tokometer may not be running, may be showing Codex, or has not read usage in the last $((MAX_AGE_S / 60)) minutes."
  exit 0
fi
# Undo JSON escapes (\" and \\).
str_field summary "$file" | sed 's/\\\(.\)/\1/g'
echo "Level: $(str_field level "$file") (warn at 80%, or from 50% when on pace to run out before reset; critical at 95%). Read $(((now - best) / 60)) min ago."
