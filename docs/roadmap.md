# Roadmap

tamias v1 is planned in the Epic [#1](https://github.com/GravionLabs/tamias/issues/1). This page names the goal of each milestone and its Features; progress, open PBIs and dependencies live on GitHub in the [milestones](https://github.com/GravionLabs/tamias/milestones) and in the native sub-issues, not here.

Work lands on `release/next`; a milestone is released from it once all its issues are closed.

## 0.1.0 – Usable CLI

Goal: one collector runs every 5 minutes on Linux, Windows and macOS, and `tamias today` (also as `--json` and `--short`) shows Claude and Copilot usage against the daily budget. Consumers can build on the stable schema v1.

| Feature | Content |
|---|---|
| [#2](https://github.com/GravionLabs/tamias/issues/2) Verified usage sources (spike) | Claude on macOS done; Linux, Windows and status line ([#20](https://github.com/GravionLabs/tamias/issues/20)), seven-day reset observed ([#18](https://github.com/GravionLabs/tamias/issues/18)), Copilot ([#17](https://github.com/GravionLabs/tamias/issues/17)), ADRs accepted and this roadmap ([#21](https://github.com/GravionLabs/tamias/issues/21)) |
| [#4](https://github.com/GravionLabs/tamias/issues/4) Claude usage and daily budget in the terminal | Workspace and CI ([#11](https://github.com/GravionLabs/tamias/issues/11)), SQLite store ([#12](https://github.com/GravionLabs/tamias/issues/12)), daily usage and budget ([#13](https://github.com/GravionLabs/tamias/issues/13)), Claude provider ([#14](https://github.com/GravionLabs/tamias/issues/14)), `collect` and `today` ([#15](https://github.com/GravionLabs/tamias/issues/15)) |
| [#5](https://github.com/GravionLabs/tamias/issues/5) Copilot usage and daily budget | Copilot provider after the spike [#17](https://github.com/GravionLabs/tamias/issues/17) |
| [#6](https://github.com/GravionLabs/tamias/issues/6) Scheduled collection on all platforms | `tamias service install`, `uninstall`, `status` (launchd agent, systemd user timer, Task Scheduler) |
| [#7](https://github.com/GravionLabs/tamias/issues/7) Claude Code status line | `tamias ingest-statusline`: fallback source and compact line |
| [#8](https://github.com/GravionLabs/tamias/issues/8) Stable consumer interface | `today --json` schema v1, `--short`, exit codes, `tamias-core` as a crate |
| [#3](https://github.com/GravionLabs/tamias/issues/3) Documentation site | VitePress on GitHub Pages from `docs/` |
| [#10](https://github.com/GravionLabs/tamias/issues/10) Releases on GitHub | CLI archives with checksums for three platforms |

Order: the spikes and the workspace ([#11](https://github.com/GravionLabs/tamias/issues/11)) first, then store, budget and Claude provider in parallel, then the CLI. Service, status line, Copilot and the consumer interface build on it; the documentation site can be done at any time.

## 0.2.0 – Desktop app

Goal: a Tauri app with a month calendar coloured by daily budget, an intraday view per day and a tray icon on all three platforms.

| Feature | Content |
|---|---|
| [#9](https://github.com/GravionLabs/tamias/issues/9) Desktop app | Angular and helix-ui; calendar, day view, tray, optional autostart |
| [#10](https://github.com/GravionLabs/tamias/issues/10) Releases (follow-up) | Tauri bundles in the release workflow |

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
