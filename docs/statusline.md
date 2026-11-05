# Claude Code status line

`tamias ingest-statusline` is a `statusLine` command for Claude Code. Claude Code runs it after each update and passes a JSON document on stdin; tamias stores the `rate_limits` from it and prints the same line as `tamias today --short`, so the usage shows in the status line of Claude Code.

## Set up

Add the command to `~/.claude/settings.json` (Windows: `%USERPROFILE%\.claude\settings.json`):

::: code-group

```json [Linux and macOS]
{
  "statusLine": {
    "type": "command",
    "command": "tamias ingest-statusline"
  }
}
```

```json [Windows]
{
  "statusLine": {
    "type": "command",
    "command": "tamias.exe ingest-statusline"
  }
}
```

:::

On Windows use `tamias.exe`, not `tamiasw`: the status line needs the output on stdout, and `tamiasw` is the build without a console for the scheduled collect. `tamias` must be on the `PATH` of Claude Code (see [Installation](./installation.md)).

## What it does

- It is a fallback source for Claude. While sessions run, the limits that Claude Code reports keep Claude `ok` in `tamias today`, even when the OAuth poll of `tamias collect` fails or did not run.
- It stores a window again only when its values changed or the last stored value is older than 5 minutes, so it stays cheap on every refresh.
- It never breaks the status line: errors are logged at `debug` level only, the line is printed from what can be read (or stays empty) and the exit code is always 0. It makes no network requests.

## The line

The line is the one of [`tamias today --short`](./consumers.md#tamias-today-short):

```text
C 62% · CP 41%
```

`C` is Claude (seven-day used %), `CP` is Copilot (monthly used %). A stale provider shows `?` (`C ?`).

The segments are coloured with ANSI colours (green, yellow, red, grey), because Claude Code renders them but stdout is no terminal there. The default is therefore `--color always`, unlike `today --short`. For a plain line:

```json
{
  "statusLine": {
    "type": "command",
    "command": "tamias ingest-statusline --color never"
  }
}
```

## No Claude part

Claude Code reports no limits with an API key, Bedrock or Vertex (`rate_limits_available: false`, or no `rate_limits`). Nothing is stored then, and the line shows a Claude part only if the OAuth poll of `tamias collect` has data for it. The Copilot part is not affected. With Claude switched off in the config nothing is stored either.

## Combine with another status line command

Claude Code runs one command. To keep an existing one, read stdin once and pass it to both:

```sh
#!/bin/sh
# ~/.claude/statusline.sh
input=$(cat)
printf '%s\n' "$input" | your-existing-statusline-command
printf '%s\n' "$input" | tamias ingest-statusline
```

Both lines are printed. Use the script in the settings:

```json
{
  "statusLine": {
    "type": "command",
    "command": "sh ~/.claude/statusline.sh"
  }
}
```
