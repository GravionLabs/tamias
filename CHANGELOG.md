# Changelog

All notable changes to GravionLabs/tamias are documented here.
Versioning follows [Semantic Versioning](https://semver.org/).

---

## [0.5.0] — 2026-11-05

### Bug Fixes
- **app**: Tray card stays hidden after a click, show-calendar event, docs (#149)
- Windows: setup prompt missing in the calendar view after start (#148) (#152)
- **app**: Show the service status error in the calendar instead of an empty view
- **app**: Never leave the calendar silently empty without data (#148)


### Features
- **app**: Tray hover shows a compact usage card (#149) (#150)
- **app**: Hovering the tray icon shows a compact usage card (#149)
- **app**: Today row and restyled open button on the tray card (#149)


## [0.4.0] — 2026-11-05

### Bug Fixes
- Day view charts grow with the window (#143) (#147)


### Documentation
- **site**: Drop roadmap and ADR links, add icon, calendar screenshots and CLI page (#142) (#146)
- Update CHANGELOG.md for v0.4.0 [skip ci]


### Features
- **app**: Week starts on Monday by default, configurable in config.toml (#144)


## [0.3.1] — 2026-11-05

### Bug Fixes
- Release build of the app loads no component styles (#138) (#139)
- **app**: Release build lost component styles because critical CSS inlining made the CSP ignore 'unsafe-inline'


### Documentation
- Update CHANGELOG.md for v0.3.1 [skip ci]


## [0.3.0] — 2026-11-05

### Bug Fixes
- Gate fake gh helper by platform and expect schema version 2 in app test (#92)
- Autostart enabled in a dev build registers the debug executable (#130) (#133)
- **app**: Refuse autostart in a dev build, which cannot run on its own after a reboot
- **app**: Satisfy clippy for autostart command (#130)
- **app**: Keep the weekend tint on future and no-data cells (#126)
- App today/month/tray use the shared Copilot availability of the CLI (#135) (#136)


### Documentation
- Copilot as a polled provider (#94) (#121)
- Copilot as a polled provider (#94)
- Fix review findings of Copilot docs; service status shows Copilot's last run (#94)
- Correct logged-out gh behaviour under copilot=auto; test service status copilot run (#94)
- Claude Code status line page (#97) (#129)
- Add Claude Code status line page (#97)
- Statusline Claude part needs OAuth credentials, Windows note (#97)
- Update CHANGELOG.md for v0.3.0 [skip ci]


### Features
- **core**: Copilot provider with the gh token source (#91) (#116)
- **cli**: Collect polls Copilot, unavailable reason in the store (#92) (#118)
- **cli**: Collect polls Copilot, unavailable reason in the store
- **cli**: Today shows Copilot and why it is unavailable (#120)
- Tamias ingest-statusline stores Claude rate limits from the status line (#96) (#122)
- Month_usage on the binding windows of today --json (#119) (#123)
- **app**: Compact month navigation in the calendar header (#125) (#132)
- **app**: Redesigned day cells in the month calendar (#126) (#134)
- **app**: Redesigned day cells in the month calendar
- **app**: App icon for window, bundles and tray (#137)


## [0.2.0] — 2026-11-04

### Bug Fixes
- **app**: Day chart ticks at real local hours on DST days, strict date, calendar a11y (#85)
- **app**: Gate the Path import to Unix, unused on Windows (#89)
- Cargo.lock locks tamias-app at 0.1.0 against workspace 0.1.2, so --locked fails after the release merge (#111) (#112)
- Lock tamias-app at the workspace version after merging main (#111)
- **ci**: Check Cargo.lock against main on release branches (#111)


### Documentation
- Desktop app page and roadmap 0.3.0 (#90) (#110)
- Desktop app page and roadmap 0.3.0
- Fix stale, colour limits, tray and Linux details on the app page (#90)
- Update CHANGELOG.md for v0.1.0 [skip ci]
- Update CHANGELOG.md for v0.1.1 [skip ci]
- Update CHANGELOG.md for v0.1.2 [skip ci]
- Update CHANGELOG.md for v0.2.0 [skip ci]


### Features
- **app**: Tauri app scaffold with Angular, helix-ui themes and the today bridge (#101)
- **app**: Live refresh when the collector writes new snapshots (#83) (#102)
- **app**: Live refresh when the collector writes new snapshots (#83)
- **app**: Month calendar coloured by daily budget, Claude and Copilot side by side (#84) (#103)
- **app**: Month calendar coloured by daily budget, Claude and Copilot side by side (#84)
- **app**: Autostart setting, off by default (#87) (#104)
- **app**: Day view with the intraday curve and the budget line (#85) (#106)
- **app**: Day view with the intraday curve and the budget line (#85)
- **app**: Tray icon with traffic light and summary, close hides to tray (#107)
- **app**: Bundled CLI sidecar and collection setup from the app (#89) (#109)
- **app**: Bundled CLI sidecar and collection setup from the app (#89)


### Refactoring
- **core**: Move config.toml parsing to tamias-core (#81) (#99)


## [0.1.2] — 2026-11-04

### Bug Fixes
- **cli**: Align exit code descriptions of help text and consumers docs (#74)
- **cli**: A fatal collect error no longer makes Claude stale or stores the config path (#76)
- **cli**: Write one error line per run when record_error fails (#77)
- **cli**: Drop Copilot from help text, untangle Copilot wording in docs (#78)
- Fatal-error tests depend on the developer's config and cover service status only on macOS (#72) (#79)


### Documentation
- ADRs are not published on the GitHub Pages site (#73) (#80)
- **site**: Do not publish the ADRs on the documentation site
- Update CHANGELOG.md for v0.1.2 [skip ci]


## [0.1.1] — 2026-11-03

### Bug Fixes
- **cli**: Log fatal errors of a scheduled collect run (#58) (#64)
- **cli**: Keep status tests and clippy green (#59)
- **schema**: Published today-v1 schema allows additional properties (#60) (#66)
- **release**: Require tamiasw.exe in Windows archives, align docs (#61) (#67)


### Documentation
- **cli**: Exit codes of collect, shared exit constants (#57) (#62)
- Copilot is announced but not polled yet (#59) (#65)
- **cli**: State that Copilot is not polled yet (#59)
- **schema**: Published today-v1 schema allows additional properties (#60) (#66)
- Update CHANGELOG.md for v0.1.1 [skip ci]


## [0.1.0] — 2026-11-03

### Bug Fixes
- **cli**: Parse the usage URL override and reject userinfo or non-loopback hosts (#15)
- **core**: Pin fixture line endings to LF (#25)
- **cli**: Service and collect-log tests use the exit codes of #26 (#28)
- **cli**: Compile the service tests outside macOS, plist mode 644, lazy store (#28)
- **cli**: Gate run_checked to macOS and tests so Linux clippy is clean (#29)
- **cli**: Restrict the unsupported-platform service test to non-macOS non-Linux (#29)
- **cli**: Gate the predicates import off Linux in the service tests (#29)
- **cli**: Windows clippy warnings, remove stray indows.rs


### Documentation
- Add ADRs for the collector architecture and the documentation site
- Accept ADR 0001 and 0002, add the roadmap (#21) (#22)
- **adr**: Accept ADR 0001 and 0002, record the spike results (#21)
- Add the roadmap and link it from the README (#21)
- **sources**: Record the observed seven-day reset, round resets_at to the minute (#18) (#24)
- Landing page for the documentation site (#35)
- **core**: Consumers page, documented tamias-core, checked Report::parse (#27) (#49)
- Installation, scheduled collection (#31) (#55)
- Installation, download and first start (#34) (#56)
- Update CHANGELOG.md for v0.1.0 [skip ci]


### Features
- **core**: SQLite snapshot store and OS data directory (#12) (#37)
- **core**: Add the shared Snapshot type (#12)
- **core**: Resolve the OS data and config directory (#12)
- **core**: Add the SQLite snapshot store with migrations (#12)
- **core**: Derive daily usage and dynamic daily budget (#13) (#38)
- **core**: Claude provider with OS credential sources (#14) (#39)
- **core**: Add the Claude provider with OS credential sources (#14)
- **site**: VitePress site from docs/ (#35) (#40)
- **site**: VitePress site from docs/ with ADR sidebar and repo links (#35)
- **cli**: Tamias collect and tamias today (#15) (#43)
- **core**: Short error class of the Claude provider for the source status (#15)
- **cli**: Tamias collect and tamias today with config.toml (#15)
- **cli**: Today --short and exit codes 0/1/3 for today and collect (#26) (#46)
- **cli**: Today --json with schema v1 (#25) (#48)
- **core**: Report model for today with serde types, schema v1 (#25)
- **cli**: Tamias today --json with JSON Schema v1 (#25)
- **cli**: Tamias service on macOS (launchd agent) and the collect log (#28) (#47)
- **cli**: Tamias service on macOS (launchd agent) and the collect log (#28)
- **cli**: Tamias service on Linux (systemd user timer) (#29) (#50)
- **cli**: Tamias service on Linux via a systemd user timer (#29)
- **cli**: Tamias service on Windows (Task Scheduler, windowless tamiasw) (#30) (#54)
- **cli**: Tamias service on Windows (Task Scheduler, windowless tamiasw) (#30)



