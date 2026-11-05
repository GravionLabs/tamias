---
layout: home

hero:
  name: tamias
  text: Your AI usage against today's budget
  image:
    src: /icon.svg
    alt: tamias icon
  tagline: How much of your Claude and GitHub Copilot usage limits you used today, and whether that fits the daily budget left until the next reset.
  actions:
    - theme: brand
      text: Usage sources
      link: /sources
    - theme: alt
      text: Command line
      link: /cli

features:
  - title: One collector
    details: Polls Claude and GitHub Copilot every 5 minutes as a user-level scheduled job on Linux, Windows and macOS, and stores snapshots in SQLite.
  - title: A dynamic daily budget
    details: What is left until the next reset, divided by the days left. Today is green below 80 %, yellow from 80 to 100 %, red above, grey when the data is stale.
  - title: Thin consumers
    details: "tamias today --json, the desktop app, poimen, herdr plugins, the Claude Code status line and skills all read the same numbers."
---

## What tamias looks like

![The month calendar of the tamias app: each day with Claude and Copilot usage chips coloured against the daily budget](/screenshots/calendar-light.png){.screenshot .light-only}

![The month calendar of the tamias app: each day with Claude and Copilot usage chips coloured against the daily budget](/screenshots/calendar-dark.png){.screenshot .dark-only}

The [desktop app](./app.md) colours every day by the daily budget; the same numbers are on the [command line](./cli.md) (`tamias today`). The screenshots show synthetic demo data.

## What tamias shows

tamias (Greek ταμίας, treasurer) shows, per provider, how much of the usage limit you used today and how that compares with today's budget. Claude's binding limit is the seven-day window, with the five-hour window as additional information; Copilot's limit is the monthly quota. A source that fails or changes is shown as `stale`, never as silently old numbers.

tamias is in development. Read on:

- [Installation](./installation.md): download, first start and scheduled collection on macOS, Linux and Windows.
- [Desktop app](./app.md): download, first start, the calendar, the day view, the tray and autostart.
- [Command line](./cli.md): every `tamias` command with a sample output.
- [Consumers](./consumers.md): how to read tamias from scripts, plugins and Rust.
- [Usage sources](./sources.md): where the numbers come from and how stable each source is.

