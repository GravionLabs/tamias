# Changelog

All notable changes to GravionLabs/tamias are documented here.
Versioning follows [Semantic Versioning](https://semver.org/).

---

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



