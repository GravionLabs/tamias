# ADR 0001: Usage collector architecture

- Status: Accepted
- Date: 2026-11-02

## Context

The owner works with Claude (Pro/Max subscription, Claude Code) and GitHub Copilot (personal seat, possibly managed by an enterprise). Both have usage limits, and neither shows reliably, in one place, how much was used today and whether that fits the budget left until the next reset.

The data sources differ in how official they are:

- **Claude.** There is no public REST API for subscription usage. Claude Code passes `rate_limits.five_hour` and `rate_limits.seven_day` (`used_percentage`, `resets_at` in epoch seconds) to a custom `statusLine` command, but only while a session runs, and an issue reports the fields missing in v2.1.96. The OAuth usage endpoint that `/usage` calls is undocumented (it reports `utilization` and ISO-8601 reset times) but can be polled without a session. The Usage & Cost Admin API covers API organizations, not subscription windows.
- **Copilot.** `copilot_internal/user` reports the personal quota and reset date with the user's own token, whoever pays for the seat; it is undocumented. The official billing endpoint `/users/{username}/settings/billing/premium_request/usage` only covers personally billed usage and returns nothing for an enterprise-managed seat; the enterprise endpoint needs admin rights.
- **No per-day value.** Both sources report a window state (percentage or remaining quota), not usage per day.

Several consumers want the same numbers: poimen (Tauri GUI over herdr), herdr plugins, the Claude Code status line, skills that should check the budget before large runs, and an overview of its own. It must run on Linux, Windows and macOS.

## Decision

- **A repository of its own, `GravionLabs/tamias`** (Greek ταμίας, treasurer), independent of poimen, so the collector and CLI work without any GUI.
- **One collector, thin consumers.** Only the collector talks to the sources. It stores snapshots in SQLite; every consumer reads through the CLI (`tamias today --json`, stable schema) or, in Rust, through `tamias-core`. No consumer polls a source itself.
- **Cargo workspace:** `crates/core` (providers, store, budget calculation), `crates/cli` (`collect`, `today`, `--json`, `service install|uninstall`), `app/` (Tauri: month calendar and tray icon). poimen uses `tamias-core` as a git dependency.
- **Sources.**
  - Claude: primary the OAuth usage endpoint; fallback the `statusLine` hook, which writes `rate_limits` snapshots into the same store (`tamias ingest-statusline`).
  - Copilot: primary `copilot_internal/user`; the official billing API is used for reconciliation only where it returns data (personally billed seats).
  - A failing or changed source is a visible state (`stale` since a timestamp), never silently old numbers.
- **Collector runs as a user-level, scheduled one-shot**, every 5 minutes, installed by `tamias service install`:
  - macOS: launchd **agent** (`~/Library/LaunchAgents`, `StartInterval`),
  - Linux: systemd `--user` timer with `Persistent=true`,
  - Windows: Task Scheduler task in the user's context, repeating every 5 minutes, "run as soon as possible after a missed start".
- **OS-specific code behind traits**, selected with `cfg(target_os)`: credential source (macOS Keychain item `Claude Code-credentials`; `~/.claude/.credentials.json` on Linux and Windows; `gh auth token` for Copilot), scheduler installer, data directory (`~/Library/Application Support/tamias`, `$XDG_DATA_HOME/tamias`, `%APPDATA%\tamias`).
- **Per-day usage is derived** from consecutive snapshots as a high-water mark: within a window (identified by `resets_at`, rounded to the nearest minute because the source jitters by about a second) only the running maximum of `used` counts, and today's usage is the rise of that maximum within the local calendar day; a jump of `resets_at` starts a new window. A lower value after a higher one (sources mixed, rounding) never counts as usage. On the day of a reset, today's budget is the old window's budget plus the new window's daily budget prorated by the rest of the day. The binding Claude limit is the seven-day window; the five-hour window is shown as additional information. Copilot's limit is the monthly quota, reset on the 1st.
- **Day boundary: local calendar day** (local time of the machine).
- **Daily budget is dynamic:** `remaining / remaining days until reset` (remaining days include today), computed at the start of the local day.
- **Colours** by today's usage relative to today's budget: green below 80 %, yellow 80–100 %, red above 100 %, grey for stale or failing data. The thresholds are configurable.
- **Overview in Tauri:** a month calendar with each day coloured, Claude and Copilot side by side; a day opens its intraday curve with the budget line. The tray icon (available on all three platforms) shows the current traffic light. Status line and herdr plugins show a compact text such as `C 62% · CP 41%`.

## Consequences

- **Reliability through statelessness.** Each collector run starts clean; a failed run is followed by an independent one, and the OS schedulers catch up after sleep. No crash recovery, wake detection or memory growth to handle.
- **No push.** Consumers see a new value at most 5 minutes late; the Tauri app watches the SQLite file. A later `tamias daemon` (long-running user process with adaptive polling and local IPC) can be added on the same `tamias-core` if live updates become necessary.
- **Undocumented endpoints.** Both primary sources can change without notice. For an enterprise-managed Copilot seat there is no official fallback, so the `stale` state carries that risk; provider code is isolated so a fix touches one module.
- **Derived daily values depend on snapshot density.** Gaps (machine off) attribute the usage to the day of the next snapshot. The model relies on the Claude seven-day window being anchored (see Follow-up); should it ever become rolling, the derivation needs a different model and a new ADR.
- **Credentials are read, never stored.** tamias reads the tokens of Claude Code and `gh` at each run and does not copy them into its own store.
- **Three platforms to test.** The scheduler installers and credential sources need a CI matrix (Linux, Windows, macOS) and a manual check on each.

## Alternatives

- **Each consumer polls on its own** (poimen, plugin, status line separately): inconsistent numbers, multiple pollers against undocumented endpoints, duplicated code. Rejected.
- **Collector as a module of poimen:** the numbers would exist only while poimen runs, and herdr plugins and skills would depend on the GUI. Rejected.
- **System-level service (Windows service, launchd daemon, systemd system service):** runs outside the user session; the macOS login keychain is locked there, a Windows service does not see the user's profile and credentials without running under the user's account with a stored password, and installation needs admin rights. Rejected.
- **Long-running user-level daemon from the start:** adaptive polling and push to the UI, but lifecycle, crash, sleep and wake handling for a budget that changes on a scale of minutes. Deferred, see Consequences.
- **Status line as the only Claude source:** gaps whenever no Claude Code session runs, and the reported regression of the fields. Kept as fallback only.
- **Static daily budget** (100 % / 7, quota / days of month): does not account for underuse on earlier days. Rejected in favour of the dynamic budget.
- **SwiftBar menu bar plugin:** macOS only. Replaced by the Tauri tray icon.

## Follow-up

Spike results, recorded in [`docs/sources.md`](../sources.md):

- **OAuth usage endpoint:** `GET /api/oauth/usage` verified on macOS, also from a launchd agent without a Keychain dialog (via `/usr/bin/security`). tamias never refreshes the token; an expired token makes Claude `stale` until Claude Code refreshes it. Linux and Windows: [#20](https://github.com/GravionLabs/tamias/issues/20).
- **Seven-day window:** anchored, not rolling: `window_started_at` is exactly seven days before `resets_at`, and `resets_at` stays constant until the reset. An observed reset confirmed it ([#18](https://github.com/GravionLabs/tamias/issues/18)): utilization dropped to 0 and `resets_at` moved by exactly seven days; the five-hour window is independent of it.
- **Status line `rate_limits`:** schema confirmed from the Claude Code 2.1.316 bundle; a real capture follows in [#20](https://github.com/GravionLabs/tamias/issues/20).
- **Copilot sources:** [#17](https://github.com/GravionLabs/tamias/issues/17).
