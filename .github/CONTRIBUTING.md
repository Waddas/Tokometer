# Contributing to Tokometer

Thanks for taking the time to contribute! Tokometer is a small, friendly
project and contributions of all sizes are welcome — bug reports, docs fixes,
new mascots, and features alike.

## Ways to help

- **Report a bug** — open an [issue](https://github.com/Waddas/Tokometer/issues)
  with steps to reproduce, your OS, and what you expected to happen.
- **Suggest a feature** — open an issue describing the problem you'd like
  solved. Discussing before building saves everyone time.
- **Send a pull request** — for anything beyond a trivial fix, please open an
  issue first so we can agree on the approach.

## Development setup

You'll need Node.js + npm and the Rust toolchain ([`rustup`](https://rustup.rs/)),
plus the platform build dependencies for Tauri — see the
[Tauri prerequisites guide](https://tauri.app/start/prerequisites/).

```sh
npm install
npm run tauri dev
```

This project uses [Task](https://taskfile.dev/) for common workflows. Run
`task` to list everything; the ones you'll use most:

| Command | What it does |
| --- | --- |
| `task dev` | Run the full app (Rust + webview) in dev mode |
| `task test` | Run the frontend (Vitest) **and** Rust test suites |
| `task check` | Type-check, lint (Clippy), and run every test |
| `task fmt` | Format the Rust source |
| `task build` | Build a production bundle for your platform |

## Before you open a pull request

- **Run `task check`** and make sure it passes — CI runs the same suites.
- **Keep changes focused.** One logical change per PR is easier to review.
- **Match the surrounding style.** Code should follow SOLID/KISS/DRY and read
  like the code around it.
- **Update docs** (README, comments) when behaviour changes.

### Commit and PR titles

Merges are squashed, and the **PR title** becomes the commit message that
[release-please](https://github.com/googleapis/release-please) reads to cut
releases. PR titles must follow
[Conventional Commits](https://www.conventionalcommits.org/), e.g.:

```
feat: add a hamster mascot
fix: correct the 7-day reset countdown across DST
docs: clarify the Windows build prerequisites
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`. A `feat` bumps the minor version, a `fix`
bumps the patch version, and a `!` (e.g. `feat!:`) marks a breaking change.
Individual commits on your branch don't need to follow the convention — only
the PR title is checked.

### Release channels

`main` is the stable channel. The long-lived `beta` branch is the pre-release
channel: PRs merged there ship as `X.Y.Z-beta.N` GitHub prereleases (with
their own `CHANGELOG-beta.md`) for users who opt in to beta updates. Target
`beta` when a change should bake in a beta before going stable.

Graduation is scripted — never open a beta→main PR. The repo is squash-only,
so the GitHub UI would collapse the whole beta cycle into a single commit and
release-please on `main` would lose the per-PR changelog entries and version
bumps. Instead a repo admin runs:

1. `task graduate` — merges `beta` into `main` with a merge commit (the push
   relies on the main ruleset's admin bypass) and release-please opens the
   stable release PR.
2. After that release PR is merged and the release publishes:
   `task sync-beta` — merges `main` back into `beta` and resets
   `.release-please-manifest-beta.json` to the new stable version so the next
   beta bumps from it.

## Code of Conduct

By participating, you agree to abide by our
[Code of Conduct](CODE_OF_CONDUCT.md). Be kind.
