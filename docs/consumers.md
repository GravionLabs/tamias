# Consumers

How other programs read tamias. The collector fills the SQLite store; `tamias today` turns it into one document. Everything a consumer needs is in that document, so no consumer reads the database.

| Need | Use |
| --- | --- |
| A script, plugin or skill | the CLI: `tamias today --json` or `--short` |
| A Rust program | the CLI plus `tamias-core` to parse the output |

## `tamias today --json`

Prints one JSON document, never coloured, described by the [JSON Schema](https://github.com/GravionLabs/tamias/blob/main/docs/schema/today-v1.schema.json) (an [example](https://github.com/GravionLabs/tamias/blob/main/docs/schema/today-v1.example.json) is next to it).

```json
{
  "schema_version": 1,
  "generated_at": "2026-11-03T10:05:00Z",
  "providers": [
    {
      "provider": "claude",
      "status": "ok",
      "windows": [
        {
          "window": "seven_day",
          "binding": true,
          "unit": "percent",
          "used": 54.0,
          "limit": 100.0,
          "resets_at": "2026-11-07T12:00:00Z",
          "colour": "green",
          "today_usage": 4.0,
          "budget": 10.0,
          "ratio": 0.4
        }
      ]
    }
  ]
}
```

### Fields

| Field | Meaning |
| --- | --- |
| `schema_version` | Version of the format, `1`. Check it first. |
| `generated_at` | When the report was made (RFC 3339, UTC). |
| `providers[].provider` | `claude` or `copilot`. |
| `providers[].status` | `ok`; `stale` (last collect failed or is older than `stale_after`); `unavailable` (not set up, no windows). |
| `providers[].stale_since` | Last successful collect of a stale provider; absent when there never was one. |
| `windows[].window` | `seven_day` and `five_hour` (Claude), `monthly` (Copilot). |
| `windows[].binding` | Whether the window has a daily budget: Claude `seven_day` and Copilot `monthly`. |
| `windows[].unit` | `percent` or `requests`. |
| `windows[].used`, `limit` | High-water mark of the current window and its limit, in `unit`. `used` is 0 once the reset has passed. |
| `windows[].resets_at` | End of the most recently observed window. |
| `windows[].colour` | `green` below 80 % of the daily budget, `yellow` 80 to 100 %, `red` above, `grey` when the provider is stale. |
| `windows[].today_usage`, `budget`, `ratio` | Usage of the local day, the budget of the local day and `today_usage / budget`. Only on binding windows, once a day can be derived; `ratio` is absent when the budget is 0. |

A window without any snapshot is left out. Optional fields are absent, never `null`.

## `tamias today --short`

One line for status lines and plugins, the binding window per provider:

```text
C 62% · CP 41%
```

`C` is Claude (seven-day used %), `CP` is Copilot (monthly used %). A stale provider shows `?` (`C ?`); an unavailable provider is left out. `--color` controls ANSI colours; the default colours only a terminal without `NO_COLOR`. `--short` and `--json` cannot be combined.

## Exit codes

| Code | `tamias collect` (and `tamiasw`) | `tamias today` |
| --- | --- | --- |
| 0 | every enabled provider was read (or none is available) | every set-up provider is ok (unavailable providers do not count) |
| 1 | tamias could not run (invalid config, unreadable store) | tamias could not run (invalid config, unreadable store) |
| 2 | invalid command line | invalid command line |
| 3 | at least one provider failed to poll; the error is recorded in the store and the collect log | at least one set-up provider is stale |

`tamiasw` is `collect` without a console window and uses the same codes. With 3 from `today` the output is still printed in full, so a consumer can show the stale state itself instead of failing. Only 1 and 2 print nothing on stdout.

## Compatibility rule

- Within `schema_version` 1, fields are only **added**, and always as optional. Ignore fields you do not know.
- A rename, a removal or a change of meaning needs `schema_version` 2. Check `schema_version` and refuse a document you do not know.

## Examples

### poimen (Rust)

Add `tamias-core` as a git dependency:

```toml
[dependencies]
tamias-core = { git = "https://github.com/GravionLabs/tamias", tag = "v0.1.0" }
```

Run the CLI and parse its output with `Report::parse`. It checks `schema_version` and rejects anything but 1 with `ParseError::UnsupportedVersion`; plain `serde_json::from_str` would not. A complete program is `crates/core/examples/read_report.rs` (`cargo run -p tamias-core --example read_report`; it is compiled by CI):

```rust
use tamias_core::report::Report;
use tamias_core::snapshot::{Provider, Window};

let output = std::process::Command::new("tamias").args(["today", "--json"]).output()?;
let report = Report::parse(&String::from_utf8(output.stdout)?)?;
let ratio = report
    .providers
    .iter()
    .find(|p| p.provider == Provider::Claude)
    .and_then(|p| p.windows.iter().find(|w| w.window == Window::SevenDay))
    .and_then(|w| w.ratio);
```

Things to know:

- `Report`, `report::Provider` and `report::Window` are `#[non_exhaustive]`, so a field added within version 1 does not break your build. You read them; you do not build them with a struct literal (`Report::new` exists for producers).
- `report::Provider` and `report::Window` share their names with `snapshot::Provider` and `snapshot::Window`, which the fields `provider` and `window` use. Import one pair with an alias (`use tamias_core::report::Window as WindowReport;`).

### herdr plugin (CLI `--short`)

A plugin that shows the line in a pane or status bar:

```sh
line=$(tamias today --short --color never)   # exit 3 = stale, the line is still printed
echo "${line:-tamias: no provider set up}"
```

### Skill (CLI `--json` with `jq`)

Check the Claude ratio before a large run:

```sh
ratio=$(tamias today --json | jq -r '
  .providers[] | select(.provider == "claude") | .windows[]
  | select(.window == "seven_day") | .ratio // empty')
if [ -n "$ratio" ] && [ "$(echo "$ratio >= 1" | bc -l)" = 1 ]; then
  echo "Over today's Claude budget (${ratio}); ask before a large run." >&2
fi
```

An empty `ratio` means no budget yet (no data, or a budget of 0): do not block on it.
