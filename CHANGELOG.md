# Changelog

## [1.5.0](https://github.com/Waddas/Tokometer/compare/v1.4.0...v1.5.0) (2026-09-03)


### Features

* check for updates daily while running ([#23](https://github.com/Waddas/Tokometer/issues/23)) ([6d6888a](https://github.com/Waddas/Tokometer/commit/6d6888ad1a43a3b30701b0233e07904ed84e139a))
* graph every displayed limit window, cycling on right-click ([#25](https://github.com/Waddas/Tokometer/issues/25)) ([51a85ca](https://github.com/Waddas/Tokometer/commit/51a85ca078ce6e63c52fbe9771bf58911e9b6940))
* legible tray ring at low usage, isolated dev app data ([#22](https://github.com/Waddas/Tokometer/issues/22)) ([acc9ba0](https://github.com/Waddas/Tokometer/commit/acc9ba03899ef6d1a0831fe678d24b65fe483ff7))
* retire the learned usage forecast beta ([#24](https://github.com/Waddas/Tokometer/issues/24)) ([5179ee4](https://github.com/Waddas/Tokometer/commit/5179ee4cbc819000871d26563f6470e357bf7305))

## [1.4.0](https://github.com/Waddas/Tokometer/compare/v1.3.0...v1.4.0) (2026-09-02)


### Features

* check for updates without installing, then offer the release ([#19](https://github.com/Waddas/Tokometer/issues/19)) ([641b8a5](https://github.com/Waddas/Tokometer/commit/641b8a581215112f51cd476ee554cec8d7b1e5b8))
* learned usage forecast behind a beta toggle ([#18](https://github.com/Waddas/Tokometer/issues/18)) ([23c3c43](https://github.com/Waddas/Tokometer/commit/23c3c437f08ac4b6219617e547bf3c2ee05fb45f))
* settings update card with ring animation, release notes and download progress ([#21](https://github.com/Waddas/Tokometer/issues/21)) ([c590f10](https://github.com/Waddas/Tokometer/commit/c590f1001b7df9ee4c72eb2631b13f9f83cbac37))

## [1.3.0](https://github.com/Waddas/Tokometer/compare/v1.2.2...v1.3.0) (2026-07-29)


### Features

* auto-detect usage limits and render a tile per limit ([#16](https://github.com/Waddas/Tokometer/issues/16)) ([4c8dd52](https://github.com/Waddas/Tokometer/commit/4c8dd520e2d40fc4eaa9e01232af1b42d7d01003))

## [1.2.2](https://github.com/Waddas/Tokometer/compare/v1.2.1...v1.2.2) (2026-07-09)


### Bug Fixes

* exponential backoff for HTTP 429 and enable fallback probe ([#10](https://github.com/Waddas/Tokometer/issues/10)) ([f24c80e](https://github.com/Waddas/Tokometer/commit/f24c80e5e9c477145c1b150251b3b789ebf5aa35))
* skip self-update checks in dev builds ([#12](https://github.com/Waddas/Tokometer/issues/12)) ([0f86370](https://github.com/Waddas/Tokometer/commit/0f86370ce05065c680a6794bfbbd23d696fa2012))

## [1.2.1](https://github.com/Waddas/Tokometer/compare/v1.2.0...v1.2.1) (2026-07-03)


### Bug Fixes

* WebView2 settings deadlock, debounce transient poll failures, and ghost lapsed usage windows ([#8](https://github.com/Waddas/Tokometer/issues/8)) ([6b7212f](https://github.com/Waddas/Tokometer/commit/6b7212f0bd65fab3d50a8a04bbf1b6515b31a2ea))

## [1.2.0](https://github.com/Waddas/Tokometer/compare/v1.1.0...v1.2.0) (2026-07-02)


### Features

* settings window, free resize, graph hover readout, backend history, and error UX ([#6](https://github.com/Waddas/Tokometer/issues/6)) ([e7130b2](https://github.com/Waddas/Tokometer/commit/e7130b2c871f94cc71dcacbe18b4395d680b0db1))


### Bug Fixes

* anchor graph ghost line to actual usage window boundaries ([#5](https://github.com/Waddas/Tokometer/issues/5)) ([37e2f1e](https://github.com/Waddas/Tokometer/commit/37e2f1e7e0a1f1eab39863181be4a5b92cb11342))

## [1.1.0](https://github.com/Waddas/Tokometer/compare/v1.0.0...v1.1.0) (2026-06-22)


### Features

* theme-aware tray icon with selectable ring/text styles ([#2](https://github.com/Waddas/Tokometer/issues/2)) ([602a459](https://github.com/Waddas/Tokometer/commit/602a45975573a548ca7ffb0b307ce9da2667d926))


### Bug Fixes

* Prevent widget displaying off screen ([#4](https://github.com/Waddas/Tokometer/issues/4)) ([f2989f6](https://github.com/Waddas/Tokometer/commit/f2989f60c938edb2ea61d43ee895765fcb48d3be))

## 1.0.0 (2026-06-16)


### Features

* V1 release ([af62cc4](https://github.com/Waddas/Tokometer/commit/af62cc4b86f4ab61e1d748fbed480c200dce4f63))
