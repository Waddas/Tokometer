# clordgauge

A small desktop tray widget that shows your Claude Code usage (5-hour and 7-day
windows). Built with [Tauri 2](https://tauri.app/) — a Rust backend (`src-tauri/`)
and a vanilla TypeScript/Vite frontend (`src/`).

> **Unofficial.** clordgauge is a community-built tool. It is not affiliated
> with, endorsed by, or sponsored by Anthropic. "Claude" and "Claude Code" are
> trademarks of Anthropic.

## What it does

A frameless, always-available widget that lives in your system tray:

- An animated pixel-art mascot whose animations follow your usage rate, next to
  two usage tiles — the current **5-hour** session and the rolling **7-day**
  window — each with a threshold-coloured percentage and reset countdown.
  Pick between **Clawd** (default), an **Axolotl**, or a **Cat** from the tray
  menu.
- The tray icon's status bubble turns green/amber/red with your usage, and its
  tooltip shows the live percentages.
- **Drag** the widget to move it; hover for pin / refresh / hide controls.
- Six layouts — mascot left/right/top/bottom of the tiles, or tiles only
  (wide/tall) — picked from the tray menu.
- Tray menu: show/hide, layout, mascot, pin on top, start at login, refresh now,
  quit.

It reads your existing Claude Code login (see below) and polls the usage API
once a minute — no separate sign-in.

## Platform support

Runs on **macOS, Windows, and Linux**, and can be developed on any of them.

A few platform details handled by the code:

- **Credentials.** The poller reads the Claude Code OAuth token fresh on every
  poll. On **macOS** it reads from the login Keychain (`security
  find-generic-password -s "Claude Code-credentials"`); on **Windows/Linux** it
  reads `~/.claude/.credentials.json` (with `%LOCALAPPDATA%`/`%APPDATA%`
  fallbacks). See `src-tauri/src/credentials.rs`.
- **Window transparency.** The transparent, undecorated widget window needs
  Tauri's `macos-private-api` feature on macOS (`macOSPrivateApi: true` in
  `tauri.conf.json` + the matching Cargo feature). It is inert on other
  platforms. Note: a macOS build using this private API cannot ship on the Mac
  App Store, which is fine for direct distribution.
- **Bundles.** `bundle.targets` is `"all"`, so each host produces its own native
  installers (NSIS/MSI on Windows, `.app`/`.dmg` on macOS, deb/AppImage/rpm on
  Linux).

## Prerequisites

- Node.js + npm
- Rust toolchain (`rustup`)
- Platform build deps for Tauri — see the
  [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)
  (e.g. Xcode Command Line Tools on macOS, WebView2 + MSVC build tools on
  Windows, `webkit2gtk` + friends on Linux).

## Develop

```sh
npm install
npm run tauri dev
```

## Build

```sh
npm run tauri build
```

Outputs land in `src-tauri/target/release/bundle/`.

## License

[MIT](LICENSE).

The bundled font (Space Grotesk) and pixel-art are redistributed under their
own permissive licenses — see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and `src/fonts/`.
