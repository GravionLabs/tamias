# Roadmap

tamias v1 is planned in the Epic [#1](https://github.com/GravionLabs/tamias/issues/1). This page names the goal of each milestone and its Features; progress, open PBIs and dependencies live on GitHub in the [milestones](https://github.com/GravionLabs/tamias/milestones) and in the native sub-issues, not here.

Work lands on `release/next`; a milestone is released from it once all its issues are closed. The milestones on GitHub are the source of the sections above.

## 0.1.0 – Usable CLI

Goal: one collector runs every 5 minutes on Linux, Windows and macOS, and `tamias today` (also as `--json` and `--short`) shows the usage against the daily budget. Consumers can build on the stable schema v1. Closed and tagged.

| Feature | Content |
|---|---|
| [#4](https://github.com/GravionLabs/tamias/issues/4) Claude usage and daily budget in the terminal | Workspace and CI, SQLite store, daily usage and budget, Claude provider, `collect` and `today` |
| [#6](https://github.com/GravionLabs/tamias/issues/6) Scheduled collection on all platforms | `tamias service install`, `uninstall`, `status` (launchd agent, systemd user timer, Task Scheduler) |
| [#8](https://github.com/GravionLabs/tamias/issues/8) Stable consumer interface | `today --json` schema v1, `--short`, exit codes, `tamias-core` as a crate |
| [#3](https://github.com/GravionLabs/tamias/issues/3) Documentation site | VitePress on GitHub Pages from `docs/` |
| [#10](https://github.com/GravionLabs/tamias/issues/10) Releases on GitHub | CLI archives with checksums for five targets (Linux x86_64 and arm64, macOS Intel and Apple silicon, Windows x86_64) |

## 0.2.0 – Fixes and documentation

Goal: the fixes and documentation PBIs that followed 0.1.0. No Feature; the milestone is closed.

## 0.3.0 – Desktop app

Goal: a Tauri app with a month calendar coloured by daily budget, an intraday view per day and a tray icon on all three platforms, with collection set up from the app and unsigned bundles on the release page. Open.

| Feature | Content |
|---|---|
| [#9](https://github.com/GravionLabs/tamias/issues/9) Desktop app | Angular and helix-ui; calendar, day view, tray, optional autostart, collection set up from the app; the [app page](./app.md) |
| [#88](https://github.com/GravionLabs/tamias/issues/88) Releases (follow-up) | Unsigned Tauri bundles in the release workflow |

## 0.4.0 – Copilot and verified sources

Goal: Copilot is polled and shown next to Claude, the Claude Code status line is a fallback source, and the usage sources are verified on all three platforms. Open.

| Feature | Content |
|---|---|
| [#5](https://github.com/GravionLabs/tamias/issues/5) Copilot usage and daily budget | Copilot provider after the spike [#17](https://github.com/GravionLabs/tamias/issues/17) |
| [#7](https://github.com/GravionLabs/tamias/issues/7) Claude Code status line | `tamias ingest-statusline`: fallback source and compact line |
| [#2](https://github.com/GravionLabs/tamias/issues/2) Verified usage sources (spike) | Claude on Linux and Windows and the status line ([#20](https://github.com/GravionLabs/tamias/issues/20)) |

## Consumers in other repositories

Blocked by the consumer interface [#8](https://github.com/GravionLabs/tamias/issues/8); planned in their own repositories:

- [poimen#64](https://github.com/GravionLabs/poimen/issues/64) Usage budget from tamias in poimen
- [herdr-plugins#177](https://github.com/GravionLabs/herdr-plugins/issues/177) tamias usage plugin
- [skills#190](https://github.com/GravionLabs/skills/issues/190) Budget check before large runs via tamias

## Later

Not planned yet and without an issue; each becomes a Feature when it is picked up:

- Code signing and notarisation (macOS, Windows); v1 ships unsigned.
- Package managers: Homebrew, winget, `cargo install`.
- `tamias daemon`: a long-running user process with adaptive polling and push to consumers (ADR 0001, Consequences).
- Demo of the calendar with sample data on the site under `/demo/` (ADR 0002).
- Retention of old snapshots (about 1,700 rows per day with both providers), for example daily aggregates beyond a cut-off.
- Dependency automation: Dependabot for the SHA-pinned actions and Cargo, `cargo-deny` in CI.
- Remove the throwaway probes in `spikes/` once 0.1.0 is released.
