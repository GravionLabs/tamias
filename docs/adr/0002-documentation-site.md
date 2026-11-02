# ADR 0002: Documentation site: VitePress on GitHub Pages, with `docs/` as the only source

- Status: Accepted
- Date: 2026-11-02
- Builds on: [ADR 0001](0001-usage-collector-architecture.md), the documentation sites of [elysion](https://github.com/GravionLabs/elysion) (its ADR 0022) and [helix](https://github.com/GravionLabs/helix)

## Context

tamias is a public repository. Its users need to know what it shows, how to install the collector on Linux, Windows and macOS, how the daily budget and the colours are computed, and how a consumer (poimen, a herdr plugin, the status line, a skill) reads `tamias today --json`. That documentation is Markdown in `docs/`, written to be read on GitHub as well. A second copy would drift. elysion and helix already publish their `docs/` as a VitePress site on GitHub Pages with the same workflow pattern.

## Decision

- **VitePress in `site/`** (a private npm package, `pnpm`), whose `srcDir` is `../docs`: nothing is copied. tamias is a Cargo workspace; `site/` is its only Node project.
- **Contents:** `docs/index.md` (landing page), installation per platform, configuration (thresholds, polling interval), the JSON schema of `tamias today --json` and how consumers use it, the data sources and their stability, and the ADRs. The ADR sidebar is read from `docs/adr` (number and first heading), so a new ADR needs no change to the site.
- **Links to files outside the site** (README, `crates/…`, `app/…`) are rewritten to `https://github.com/GravionLabs/tamias/blob/main/<path>` by a small markdown-it plugin, taken over from elysion with its tests. VitePress's **dead link check fails the build**.
- **Published at `https://gravionlabs.github.io/tamias/`** by `.github/workflows/pages.yml`, structured like elysion's: pull requests that touch `site/`, `docs/` or the workflow build the site; only `main` deploys, and only while the repository variable `DEPLOY_PAGES` is `true`, with the Pages source set to "GitHub Actions". `SITE_BASE` is `/<repository>/`. Actions are pinned by commit SHA.
- **No demo on the site** for now. A static preview of the Tauri calendar with sample data (like helix's `/demo/`) can be added later under `/demo/`.

## Consequences

- The site is live only once the owner sets `DEPLOY_PAGES` to `true` and the Pages source to "GitHub Actions".
- tamias gets a Node toolchain beside Rust (`site/package.json`, `pnpm-lock.yaml`); the Tauri app brings one anyway.
- Documents stay plain Markdown readable on GitHub; a word in angle brackets outside code is written in code or as `&lt;name&gt;`.
- Local search of VitePress, no analytics, no external service.

## Alternatives

- **mdBook** (Rust-native): fits a Cargo workspace, but differs from elysion and helix in workflow, link handling and look. Rejected for consistency across the owner's repositories.
- **README and `docs/` on GitHub only:** no installation guide that reads like a site, no search. Rejected.
- **GitHub wiki:** a second source beside `docs/`, outside pull request review. Rejected.
