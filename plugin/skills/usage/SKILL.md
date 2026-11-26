---
name: usage
description: Current Claude usage limits (5-hour, weekly, model-specific) from the Tokometer desktop app, with reset times and pace. Use when the user asks about usage or limits, when your instructions ask you to watch usage, or before starting something large (many subagents, a long workflow, a wide review) to check there's room for it.
allowed-tools: Bash(sh "${CLAUDE_PLUGIN_ROOT}/scripts/tokometer-usage.sh")
---

# Claude usage from Tokometer

Run:

```sh
sh "${CLAUDE_PLUGIN_ROOT}/scripts/tokometer-usage.sh"
```

It prints each limit window's percentage, time until reset and, when the current pace runs it out before reset, how soon.
A second line gives the overall level (ok, warn, critical) and how old the reading is.

- **Report it plainly.** Give the numbers and reset times; don't guess at usage the output doesn't show.
- **It's information.** Don't change the plan because of it unless the user or your instructions say to.
- **No reading** means Tokometer isn't running, is showing Codex, or hasn't read usage recently. Say so; don't
  look for usage elsewhere.
