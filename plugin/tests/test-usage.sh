#!/bin/sh
# Tests for plugin/scripts/tokometer-usage.sh. Run: sh plugin/tests/test-usage.sh
# (TEST_SH=dash runs the script under another shell).
set -u
here=$(cd "$(dirname "$0")" && pwd)
script="$here/../scripts/tokometer-usage.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export TOKOMETER_USAGE_FILE="$tmp/usage-status.json"
fails=0

# Same layout as serde_json::to_string_pretty of status_file::StatusFile.
status() { # level [provider] [age_s]
  cat >"$TOKOMETER_USAGE_FILE" <<EOF
{
  "version": 1,
  "provider": "${2:-claude}",
  "status": "ok",
  "observedAt": $(($(date +%s) - ${3:-90})),
  "level": "$1",
  "summary": "Claude usage: 5h 87% (resets in 1h 12m), Fable \"beta\" 40% C:\\\\x.",
  "windows": [
    {
      "id": "session",
      "label": "5h",
      "utilization": 87.0,
      "resetsAt": 1800000000,
      "stale": false,
      "runsOutAt": null
    }
  ]
}
EOF
}

check() { # name expected-substring
  out=$(${TEST_SH:-sh} "$script")
  case $out in *"$2"*) return ;; esac
  echo "FAIL: $1"
  echo "  expected: $2"
  echo "  actual:   $out"
  fails=$((fails + 1))
}

check "no file explains there is no reading" 'No current Claude usage'

status warn
check "prints the summary unescaped" 'Claude usage: 5h 87% (resets in 1h 12m), Fable "beta" 40% C:\x.'
check "prints the level and age" 'Level: warn (warn at 80%'
check "prints the age in minutes" 'Read 1 min ago.'

status warn claude 3600
check "stale file is not used" 'No current Claude usage'
status warn codex
check "codex file is not used" 'No current Claude usage'

# Without the override, the freshest file across the prod and dev app ids wins.
prod="$tmp/xdg/com.bradleywadsworth.tokometer"
dev="$tmp/home/Library/Application Support/com.bradleywadsworth.tokometer.dev"
mkdir -p "$prod" "$dev"
status ok claude 300 && mv "$TOKOMETER_USAGE_FILE" "$prod/"
status critical claude 60 && mv "$TOKOMETER_USAGE_FILE" "$dev/"
override=$TOKOMETER_USAGE_FILE
unset TOKOMETER_USAGE_FILE
export HOME="$tmp/home" XDG_CONFIG_HOME="$tmp/xdg"
check "finds the freshest file in the app dirs" 'Level: critical'
rm "$dev/usage-status.json"
check "falls back to the other app id" 'Level: ok'
export TOKOMETER_USAGE_FILE="$override"

if [ "$fails" -eq 0 ]; then echo "usage script: all tests passed"; else exit 1; fi
