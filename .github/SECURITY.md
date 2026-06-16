# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security vulnerabilities.

Instead, report them privately using GitHub's
[private vulnerability reporting](https://github.com/Waddas/Tokometer/security/advisories/new),
or by emailing **b.wadsworth@live.co.uk**.

Please include:

- a description of the issue and its impact,
- steps to reproduce, and
- any relevant logs or proof-of-concept.

You can expect an initial response within a few days. Once a fix is available,
a release will be published and the reporter credited (unless you prefer to
remain anonymous).

## Scope

Tokometer reads your existing Claude Code OAuth credentials locally to poll
Anthropic's usage endpoint. It never stores or transmits those credentials
anywhere else. Reports relating to credential handling, the auto-updater, or
the usage poller are especially welcome.

## Supported versions

As a pre-1.0 project, only the latest released version receives security fixes.
