# Command line

The `tamias` command collects the usage, shows it and sets up the scheduled collection. The sample outputs on this page come from a throwaway config and database with synthetic numbers; yours show your own usage and dates. [Installation](./installation.md) explains how to get the CLI.

## `tamias today`

Today's usage against today's budget, one row per window:

```console
$ tamias today
PROVIDER  WINDOW     USED  TODAY  BUDGET  RESETS            STATUS
claude    seven_day  43%   6.2%   11.4%   2026-11-09 11:04  green
claude    five_hour  18%   -      -       14:04             green
copilot   -          -     -      -       -                 unavailable (not set up)
```

`USED` is the usage of the window so far, `TODAY` what you used today and `BUDGET` what today may use to stay on track until `RESETS`. `STATUS` is the colour of the day: `green` below 80 % of the budget, `yellow` up to 100 %, `red` above, `stale` when the data is old. On a terminal the table is coloured; `--color never` turns that off, `NO_COLOR` is respected.

The exit code is 0 when every set-up provider is fresh, 3 when one is stale, 1 on an error (for example an invalid config) and 2 on an invalid command line.

### `tamias today --short`

One compact line for status lines and tray tooltips: the binding window of each set-up provider.

```console
$ tamias today --short
C 43%
```

`C` is Claude, `CP` is Copilot; a stale provider shows `?` instead of the percentage, and an unavailable one is left out (`C 43% · CP 41%` with both).

### `tamias today --json`

One JSON document, never coloured, for scripts and other programs (an excerpt):

```console
$ tamias today --json
{
  "schema_version": 1,
  "generated_at": "2026-11-05T11:04:51Z",
  "providers": [
    {
      "provider": "claude",
      "status": "ok",
      "windows": [
        {
          "window": "seven_day",
          "binding": true,
          "unit": "percent",
          "used": 43.0,
          "limit": 100.0,
          "resets_at": "2026-11-09T11:04:44Z",
          "colour": "green",
          "today_usage": 6.2,
          "budget": 11.4,
          "ratio": 0.54,
          "month_usage": 6.2
        }
      ]
    }
  ]
}
```

All fields and the JSON Schema are described in [Consumers](./consumers.md).

## `tamias collect`

Polls every available provider once and stores the snapshots. It prints nothing; the scheduled job runs it every 5 minutes and writes the result of each run to `collect.log`. Run it by hand to check that collection works; `-v` shows what it does:

```console
$ tamias collect -v
DEBUG reading config /home/you/.config/tamias/config.toml
 INFO claude: stored 2 snapshots
 INFO copilot: unavailable (not set up), skipped: no GitHub credentials: gh was not found
$ echo $?
0
```

It exits with 3 when a poll failed (the provider then turns stale), see the exit codes above.

## `tamias service`

Sets up, checks and removes the scheduled collection with the scheduler of your operating system (launchd, a systemd user timer or Task Scheduler). It never needs administrator rights.

```console
$ tamias service install
installed: tamias collect runs every 5 minutes (/usr/local/bin/tamias)
log: /home/you/.local/share/tamias/logs/collect.log

$ tamias service status
service: installed
definition: /home/you/.config/systemd/user/tamias.service
exe: /usr/local/bin/tamias
claude: last run 2026-11-05 11:05 ok
copilot: last run 2026-11-05 11:05 ok
log: /home/you/.local/share/tamias/logs/collect.log

$ tamias service uninstall
uninstalled; the data and the log are kept
```

`status` before the first `install` says `service: not installed (run `tamias service install`)`. The exact paths depend on the operating system. See [Installation](./installation.md#scheduled-collection).

## `tamias ingest-statusline`

The `statusLine` command of Claude Code: it reads the rate limits from the JSON that Claude Code passes on stdin, stores them and prints the short line. It never fails and always exits with 0.

```console
$ echo '{"rate_limits":{"seven_day":{"used_percentage":43,"resets_at":1793000000}}}' | tamias ingest-statusline --color never
C 43%
```

Setting it up in Claude Code is described in [Claude Code status line](./statusline.md).

## `--config` and `-v`

Both options work with every command.

`--config <PATH>` reads that file instead of `config.toml` in the config directory, for example to try settings without touching your own:

```console
$ tamias --config ./try.toml today --short
C 43%
```

`-v` (`--verbose`) writes debug logging to stderr, so it does not disturb the output on stdout. Repeat it for more detail:

```console
$ tamias -v today --short
DEBUG reading config /home/you/.config/tamias/config.toml
DEBUG database /home/you/.local/share/tamias/tamias.db
C 43%
```
