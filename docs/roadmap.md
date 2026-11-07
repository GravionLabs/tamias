# Roadmap

tamias v1 is planned in the Epic [#1](https://github.com/GravionLabs/tamias/issues/1). This page names the goal of each milestone and its Features; progress, open PBIs and dependencies live on GitHub in the [milestones](https://github.com/GravionLabs/tamias/milestones) and in the native sub-issues, not here.

Work lands on `release/next`; a milestone is released from it once all its issues are closed. The milestones on GitHub are the source of the sections below.

Milestones are named `M<n>`; a closed one is renamed `M<n> (x.y.z)` after its release tag. The two legacy milestones `0.1.0` and `0.1.1` predate that scheme and keep their plain version names.

| Milestone | Release tag | State |
|---|---|---|
| `0.1.0` Usable CLI (legacy) | v0.1.0 | Closed |
| `0.1.1` Fixes and documentation (legacy) | v0.1.1 | Closed |
| M1 (0.2.0) Desktop app | v0.1.2 and v0.2.0 | Closed |
| M2 (0.3.0) Copilot and verified sources | v0.3.0 and v0.3.1 | Closed |
| M3 (0.4.0) Fixes and week start | v0.4.0 | Closed |
| M4 (0.6.0) Tray card and projection | v0.5.0 and v0.6.0 | Closed |
| M5 Status bar and open sources | not released yet | Open |
| M6 Usage history charts | not released yet | Open |

v0.1.2 has no milestone of its own: its fixes (#68 to #73, see the CHANGELOG) are closed issues of M1 and were released as a patch tag before the desktop app shipped in v0.2.0.

## 0.1.0 – Usable CLI (legacy)

Goal: one collector runs every 5 minutes on Linux, Windows and macOS, and `tamias today` (also as `--json` and `--short`) shows the usage against the daily budget. Consumers can build on the stable schema v1. Closed; shipped as tag v0.1.0.

| Feature | Content |
|---|---|
| [#4](https://github.com/GravionLabs/tamias/issues/4) Claude usage and daily budget in the terminal | Workspace and CI, SQLite store, daily usage and budget, Claude provider, `collect` and `today` |
| [#6](https://github.com/GravionLabs/tamias/issues/6) Scheduled collection on all platforms | `tamias service install`, `uninstall`, `status` (launchd agent, systemd user timer, Task Scheduler) |
| [#8](https://github.com/GravionLabs/tamias/issues/8) Stable consumer interface | `today --json` schema v1, `--short`, exit codes, `tamias-core` as a crate |
| [#3](https://github.com/GravionLabs/tamias/issues/3) Documentation site | VitePress on GitHub Pages from `docs/` |
| [#10](https://github.com/GravionLabs/tamias/issues/10) Releases on GitHub | CLI archives with checksums for five targets (Linux x86_64 and arm64, macOS Intel and Apple silicon, Windows x86_64) |

## 0.1.1 – Fixes and documentation (legacy)

Goal: the fixes and documentation PBIs that followed 0.1.0. No Feature; the milestone is closed and shipped as tag v0.1.1.

## M1 (0.2.0) – Desktop app

Goal: a Tauri app with a month calendar coloured by daily budget, an intraday view per day and a tray icon on all three platforms, with collection set up from the app and unsigned bundles on the release page. Closed; shipped as tag v0.2.0, preceded by the patch tag v0.1.2 with its first fixes.

| Feature | Content |
|---|---|
| [#9](https://github.com/GravionLabs/tamias/issues/9) Desktop app | Angular and helix-ui; calendar, day view, tray, optional autostart, collection set up from the app; the [app page](./app.md) |
| [#88](https://github.com/GravionLabs/tamias/issues/88) Releases (follow-up) | Unsigned Tauri bundles in the release workflow |

## M2 (0.3.0) – Copilot and verified sources

Goal: Copilot is polled and shown next to Claude, the Claude Code status line is a fallback source, and the usage sources are verified on all three platforms. Closed; shipped as tag v0.3.0, followed by the patch tag v0.3.1.

| Feature | Content |
|---|---|
| [#5](https://github.com/GravionLabs/tamias/issues/5) Copilot usage and daily budget | Copilot provider after the spike [#17](https://github.com/GravionLabs/tamias/issues/17) |
| [#7](https://github.com/GravionLabs/tamias/issues/7) Claude Code status line | `tamias ingest-statusline`: fallback source and compact line |
| [#2](https://github.com/GravionLabs/tamias/issues/2) Verified usage sources (spike) | Claude on Linux and Windows and the status line ([#20](https://github.com/GravionLabs/tamias/issues/20)) |

## M3 (0.4.0) – Fixes and week start

Goal: a release build with working styles, a configurable week start (Monday by default), larger day charts and a site with screenshots. Closed; shipped as tag v0.4.0.

## M4 (0.6.0) – Tray card and projection

Goal: a compact usage card on tray hover and a month projection in the card and the calendar, with the fixes found on the way. Closed; shipped as the tags v0.5.0 and v0.6.0.

## M5 – Status bar and open sources

Goal: a window that fits the calendar with a status bar (version, zoom, auto-update from GitHub Releases), and the Features of M2 that are still open (Copilot [#5](https://github.com/GravionLabs/tamias/issues/5), status line [#7](https://github.com/GravionLabs/tamias/issues/7), verified sources [#2](https://github.com/GravionLabs/tamias/issues/2)). Open; no tag yet.

## M6 – Usage history charts

Goal: the [#164](https://github.com/GravionLabs/tamias/issues/164) Feature: the window bars of the tray card in the calendar and the day view (the month and day charts were dropped) and a year view with a bar per month; see the [app page](./app.md#day-view). Open; no tag yet.

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
- Remove the throwaway probes in `spikes/` once v0.1.0 is released.
