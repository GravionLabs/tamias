# tamias

tamias (Greek ταμίας, treasurer) shows how much of your Claude and GitHub Copilot usage limits you used today, and whether that fits the daily budget left until the next reset.

- One collector polls the sources every 5 minutes (user-level scheduler on Linux, Windows and macOS) and stores snapshots in SQLite.
- Consumers read the same numbers: `tamias today --json`, the Tauri app (month calendar, tray icon), poimen, herdr plugins, the Claude Code status line, skills.
- Daily usage is coloured relative to the dynamic daily budget: green below 80 %, yellow 80–100 %, red above 100 %, grey when the data is stale.

Status: in development. Install: [installation](docs/installation.md). Reading tamias from your own program: [consumers](docs/consumers.md). See the [roadmap](docs/roadmap.md) and the decisions in [docs/adr](docs/adr).

## License

[MIT](LICENSE)
