---
layout: home

hero:
  name: tamias
  text: Your AI usage against today's budget
  tagline: How much of your Claude and GitHub Copilot usage limits you used today, and whether that fits the daily budget left until the next reset.
  actions:
    - theme: brand
      text: Usage sources
      link: /sources
    - theme: alt
      text: Roadmap
      link: /roadmap
    - theme: alt
      text: Decisions
      link: https://github.com/GravionLabs/tamias/blob/main/docs/adr/0001-usage-collector-architecture.md

features:
  - title: One collector
    details: Polls Claude every 5 minutes (Copilot is coming with [#5](https://github.com/GravionLabs/tamias/issues/5)) as a user-level scheduled job on Linux, Windows and macOS, and stores snapshots in SQLite.
  - title: A dynamic daily budget
    details: What is left until the next reset, divided by the days left. Today is green below 80 %, yellow from 80 to 100 %, red above, grey when the data is stale.
  - title: Thin consumers
    details: "tamias today --json, the desktop app, poimen, herdr plugins, the Claude Code status line and skills all read the same numbers."
---

## What tamias shows

tamias (Greek ταμίας, treasurer) shows, per provider, how much of the usage limit you used today and how that compares with today's budget. Claude's binding limit is the seven-day window, with the five-hour window as additional information; Copilot's limit is the monthly quota. A source that fails or changes is shown as `stale`, never as silently old numbers.

tamias is in development. Read on:

- [Installation](./installation.md): download, first start and scheduled collection on macOS, Linux and Windows.
- [Desktop app](./app.md): download, first start, the calendar, the day view, the tray and autostart.
- [Consumers](./consumers.md): how to read tamias from scripts, plugins and Rust.
- [Usage sources](./sources.md): where the numbers come from and how stable each source is.
- [Roadmap](./roadmap.md): the milestones and their Features.
- Decisions: [ADR 0001](./adr/0001-usage-collector-architecture.md) (collector architecture) and [ADR 0002](./adr/0002-documentation-site.md) (this site), on GitHub.

