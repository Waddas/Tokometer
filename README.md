<div align="center">

# Tokometer

**A tiny desktop widget that keeps your Claude Code usage in sight — with a pixel-art coworker.**

[![CI](https://github.com/Waddas/Tokometer/actions/workflows/ci.yml/badge.svg)](https://github.com/Waddas/Tokometer/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Waddas/Tokometer?display_name=tag&sort=semver)](https://github.com/Waddas/Tokometer/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB.svg)](https://tauri.app/)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue.svg)

<img src="docs/hero.png" alt="Tokometer widget showing the mascot and usage tiles" width="360" />

</div>

> **Unofficial.** Tokometer is a community-built tool. It is not affiliated
> with, endorsed by, or sponsored by Anthropic. "Claude" and "Claude Code" are
> trademarks of Anthropic.

## Features

- **Live usage at a glance** — the current **5-hour** session and rolling
  **7-day** window, each with a threshold-coloured percentage and reset
  countdown, polled once a minute from your existing Claude Code login. No
  separate sign-in.
- **A mascot that works when you do** — pixel-art animations speed up with
  your usage rate. Pick **Clawd**, an **Axolotl**, or a **Cat**.
- **Usage graph with a forecast** — click the mascot to flip it into a
  usage-over-time graph: gradient-coloured history, a dotted prediction at
  your current pace, the limit ceiling, your reset time, and a faint ghost of
  the previous window for comparison. Hover to read off the time and
  percentage at any point.
- **Stays out of the way** — frameless, draggable, freely resizable from the
  corner grip, optionally pinned above the taskbar, hidden to the tray when
  you don't want it.
- **Six layouts** — mascot/graph beside, above, or below the tiles, or tiles
  only — all in a proper settings window.

## Showcase

| Mascots at work | Usage graph |
| :---: | :---: |
| <img src="docs/mascots.gif" alt="The three mascots animating" width="360" /> | <img src="docs/graph.png" alt="Usage graph with prediction" width="360" /> |

| Animations follow your usage rate | Layouts |
| :---: | :---: |
| <img src="docs/usage-rate.gif" alt="Mascot animation speeding up with usage" width="360" /> | <img src="docs/layouts.png" alt="The six widget layouts" width="360" /> |

## Controls

| Action | What it does |
| --- | --- |
| **Drag** the widget (or the ☰ handle) | Move it anywhere |
| **Drag** the ↘ corner grip | Resize the widget to any scale |
| **Click** the mascot | Flip between mascot and usage graph |
| **Hover** the graph | Read the time and percentage under the cursor |
| **Right-click** the graph | Switch between the 5-hour and 7-day windows |
| **Right-click** the mascot | Pick a mascot (Clawd / Axolotl / Cat) |
| **Hover** | Reveal pin-on-top, refresh, settings, and hide buttons |
| **Settings window** (⚙ or tray) | Layout, size, mascot, tray icon, work days, pin, start at login |
| **Tray menu** | Show/hide, settings, refresh, check for updates, quit |

The tray icon doubles as a status light — its bubble turns green/amber/red
with your session usage, and the tooltip shows both live percentages.

## How it works

- **Credentials** — the poller reuses your Claude Code OAuth login, read fresh
  on every poll. On **macOS** that's the login Keychain
  (`Claude Code-credentials`); on **Windows/Linux** it's
  `~/.claude/.credentials.json` (with `%LOCALAPPDATA%`/`%APPDATA%` fallbacks).
  Nothing is stored or sent anywhere else.
- **Usage** — Anthropic's OAuth usage endpoint is polled once a minute for the
  utilization and reset time of both windows, with a probe fallback (below)
  when that endpoint fails. If the endpoint rate-limits the account (HTTP
  429), retries back off exponentially (2 → 4 → 8 → 15 min, with jitter)
  instead of hammering it; a manual refresh retries immediately.
- **History** — the API only reports *current* utilization, so the app
  accumulates its own time series locally (`history.json` next to its config)
  to draw the graph: full resolution for recent hours, thinned to one sample
  per five minutes beyond that, capped at 15 days — enough for each view to
  show a ghost of its previous window. Each sample records its window's reset
  time, so windows are compared by identity rather than wall-clock guesswork.
- **Fallback probe** (on by default) — if the free usage endpoint fails,
  Tokometer cross-checks by sending a minimal 1-token `/v1/messages` request
  and reading the rate-limit headers. It only runs while the usage endpoint
  is failing, never fires more than once every 5 minutes, and spends a
  sliver (one Haiku token) of the quota it measures — it can be turned off
  under Settings → Fallback usage probe.

## Install

Grab the latest installer for your platform from the
[**Releases**](https://github.com/Waddas/Tokometer/releases/latest) page:

- **macOS** — `.dmg` (universal — Intel & Apple Silicon)
- **Windows** — `.msi` or NSIS `.exe`
- **Linux** — `.AppImage`, `.deb`, or `.rpm`

Tokometer ships with an auto-updater, so you'll be prompted when a new version
is available.

> **macOS Gatekeeper / Windows SmartScreen:** the app isn't code-signed yet, so
> your OS may warn on first launch. On macOS, right-click the app → **Open**; on
> Windows, choose **More info → Run anyway**.

Prefer to build it yourself? See [Getting started](#getting-started) below.

## Getting started

### Prerequisites

- Node.js + npm
- Rust toolchain ([`rustup`](https://rustup.rs/))
- Platform build deps for Tauri — see the
  [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
  (Xcode Command Line Tools on macOS, WebView2 + MSVC build tools on Windows,
  `webkit2gtk` + friends on Linux)

### Develop

```sh
npm install
npm run tauri dev
```

Or with [Task](https://taskfile.dev/): `task install`, then `task dev`.

**Dev tips**

- Press **D** in a dev build to toggle dev mode — a small badge in the strip
  above the widget shows the current state, and leaving dev mode resets it.
  While it's on:
  - **M** cycles the data source: mocked usage data — a representative set of
    curves (bursts, plateaus, a near-limit previous window) so you can iterate
    on the graph without waiting for live history — then a mocked API failure
    (for the error status bar), then back to live. Your real local history is
    untouched throughout.
  - **A** pins the mascot to a specific animation, cycling through all of
    them and back to the automatic rate-based rotation.
- `task test` runs the frontend (Vitest) and Rust test suites; `task check`
  adds typechecking and linting.
- `task test:e2e` (Windows only) builds a debug app under a separate identifier
  and drives the real UI over the WebView2 devtools protocol — it opens the
  settings window from the widget and asserts the app stays responsive, which
  unit tests can't cover.

### Build

```sh
npm run tauri build   # or: task build
```

Native installers land in `src-tauri/target/release/bundle/` — NSIS/MSI on
Windows, `.app`/`.dmg` on macOS, deb/AppImage/rpm on Linux.

## Platform notes

- **Window transparency** needs Tauri's `macos-private-api` feature on macOS
  (already configured); it is inert elsewhere. A macOS build using this
  private API cannot ship on the Mac App Store, which is fine for direct
  distribution.
- On **Windows**, a pinned widget re-asserts itself above the taskbar, which
  shares the topmost z-band.

## Contributing

Contributions are welcome — bug reports, docs, new mascots, and features alike.
See [CONTRIBUTING.md](.github/CONTRIBUTING.md) for the development workflow and
PR conventions, and please follow the
[Code of Conduct](.github/CODE_OF_CONDUCT.md). Found a security issue? See the
[Security Policy](.github/SECURITY.md).

## Credits

- Inspired by [Clawdmeter](https://github.com/HermannBjorgvin/Clawdmeter).
- The **Clawd** pixel-art is derived from the community
  [claudepix](https://claudepix.vercel.app/) set — thank you!
- Typeface: [Space Grotesk](https://fonts.google.com/specimen/Space+Grotesk)
  (SIL OFL 1.1).

## License

[MIT](LICENSE).

The bundled font and pixel-art are redistributed under their own permissive
licenses — see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
`src/fonts/`.
