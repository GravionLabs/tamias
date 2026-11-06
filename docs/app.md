# Desktop app

The desktop app shows the same numbers as `tamias today`: a month calendar with each day coloured by the daily budget, a day view with the usage of the day and the windows, a year view with a bar per month, and a tray icon with the traffic light. It only reads. Collection stays the scheduled `tamias collect` of the CLI, which the app can set up for you ([Installation](./installation.md#scheduled-collection)).

## Download

Every release on the [GitHub releases page](https://github.com/GravionLabs/tamias/releases) has the app bundles next to the CLI archives, named `tamias-app-<version>-<target>.<ext>`, and they are listed in `SHA256SUMS`:

| Platform | Bundle |
| --- | --- |
| macOS, Apple silicon | `tamias-app-<version>-aarch64-apple-darwin.dmg` |
| macOS, Intel | `tamias-app-<version>-x86_64-apple-darwin.dmg` |
| Windows, x86_64 | `tamias-app-<version>-x86_64-pc-windows-msvc.msi` |
| Linux, x86_64 | `tamias-app-<version>-x86_64-unknown-linux-gnu.AppImage` or `.deb` |

Check the checksum as described in [Installation](./installation.md#download). Each bundle contains the `tamias` CLI of the same version (on Windows also `tamiasw.exe`), so the app does not need a separate CLI install.

The bundles are not signed or notarised by the operating system (v1 ships unsigned); they are signed for the updater only (see [Updates](#updates)). Each system warns on the first start:

- **macOS**: open the `.dmg` and drag the app into `/Applications` first, then start it from there. An unsigned app started from Downloads or straight from the mounted disk image runs from a temporary path (macOS App Translocation). Collection set up from such a start would be registered with that temporary path, so move the app first. To remove the quarantine flag of the installed app once:

  ```sh
  xattr -dr com.apple.quarantine /Applications/tamias.app
  ```

  Alternatively start it once, then open System Settings, Privacy & Security, and choose "Open Anyway" for tamias.
- **Windows**: SmartScreen may warn about an unrecognized app. Choose "More info", then "Run anyway".
- **Linux**: no warning. Make the AppImage executable (`chmod +x`) before you start it.

## First start and collection setup

On the first start without data and without a collector, the calendar asks "Collection is not set up — set up now?". "Set up" runs `tamias service install` of the bundled CLI, which registers the collector with your operating system's scheduler:

- macOS and Windows: the scheduler runs the CLI inside the installed app (`tamias.app/Contents/MacOS/tamias`, or `tamiasw.exe` in the install directory on Windows).
- Linux: the app first copies the CLI to `~/.local/bin/tamias` (replacing an older copy) and the systemd timer runs that copy. This happens on every Linux install, `.deb` included; for the AppImage it is needed because it is mounted at a new path on every start. Put `~/.local/bin` on your `PATH` if you want to run `tamias` yourself.

"Not now" hides the prompt only until the app is closed; it returns on the next start. Settings shows the collection status at any time, with "Set up" and "Remove". A collector you registered with a CLI from the archives keeps working: the app shows it as set up and does not replace it.

Every call of the CLI is limited to 10 seconds; a call that fails or does not answer is shown as an error with its message. In the calendar, without data, an unreadable status shows that message with a Retry button instead of an empty calendar. The status error shows only when there is no data; a calendar with data never shows it. A first failed status read is retried once automatically before the error appears, a status call that does not answer within 15 seconds ends as an error, and while the status is being read the calendar says so. A report whose providers are all unavailable, stale without a first success, or ok without a single day counts as no data.

"Remove" in Settings (or `tamias service uninstall`) removes the registration; it keeps the data and the log. Remove the registration before you delete the app, otherwise the scheduler keeps running a missing executable.

## Calendar

The calendar opens on the current month. Previous, Today and Next move between months. Claude (`C`) and Copilot (`CP`) are shown side by side in each day's cell as status pills with the usage of the day against the budget, for example `C 64 %`. Today's number sits on a filled badge, future days and days without data are quieter (dashed outline pills), weekends are lightly tinted, and clickable days show a hover and keyboard focus state. A chip's colour is the colour of that day against the daily budget. The limits are the defaults of `yellow_at` (0.8) and `red_at` (1.0) in `config.toml`; green is below `yellow_at`, yellow up to and including `red_at`, red above:

| Colour | Meaning |
| --- | --- |
| Green | below 80 % of the daily budget (by default) |
| Yellow | from 80 % up to and including 100 % (by default) |
| Red | above 100 % (by default) |
| Grey | today only: the provider is stale |
| Neutral | no data for that day, or the provider is not available |

Below the header the calendar shows a month projection per provider ("Hochrechnung"): the usage of the month so far divided by the elapsed days of the current local month (up to and including today), times the days in the month, as a percentage of the monthly budget (the daily budget times the days in the month). A past month shows its final usage as "month total" instead, a future month nothing. The chip's colour comes from the projection itself (projected usage / monthly budget) with the same limits as the day colours above: green below 80 %, yellow up to and including 100 %, red above (a 150 % projection is red); it is grey while the provider is stale. Providers without a budget or without data have no projection.

A provider is stale when its last successful read is older than `stale_after` (default 15 minutes, set in `config.toml`; see also [Consumers](./consumers.md)), when it has never succeeded, or when its last run failed after its last success, even if that success is younger than `stale_after`. On today's cell the grey chip says "stale since 09:00" (the time of the last successful collect), or "stale, never collected". Today's cell is only grey while the provider is stale; the calendar does not show old numbers as current.

The week starts on Monday. Set `week_start` in `config.toml` to change it: `"monday"` (default), `"sunday"`, `"saturday"` or `"locale"` (the first day of your system locale). The weekday header and the empty cells before the 1st follow it; Settings shows the active value, read only. The app reads it at start, so restart the app after changing it. It does not change how the CLI reports days or budgets.

```toml
week_start = "sunday"
```

The calendar refreshes when the collector writes new snapshots, so the app needs no reload. The first data appears with the next collect, within 5 minutes.

![The month calendar with Claude and Copilot usage chips per day](/screenshots/calendar-light.png){.screenshot .light-only}

![The month calendar with Claude and Copilot usage chips per day](/screenshots/calendar-dark.png){.screenshot .dark-only}

Click a day to open its day view, or "Year" for the year view. The "Today overview" link shows today's report of every provider.

## Status bar and zoom

The window opens at 960 x 880 and cannot get smaller than 640 x 840, so a whole month (header, projection chips, six week rows) is visible at 100 % without scrolling. The bar at the bottom of the window shows the installed version (for example `v0.6.0`) and the zoom: `−`, the percentage and `+`. The percentage is the zoom of the window, in steps of 10 % between 50 % and 200 %; click it to go back to 100 %. The zoom is remembered across restarts. At a high zoom the content scrolls above the bar. The tray card has no status bar.

The sizes were measured, not guessed: the calendar content is 783 px high at zoom 100 % (week cells 98 px high with the two projection chips wrapped; headless Chrome on the real `calendar.css`/`styles.css` with sample data), the status bar is about 28 px, and the content needs about 600 px of width. Hence the minimum of 640 x 840 and the default of 960 x 880. Fonts on macOS and Windows may differ from the headless Chrome ones, so the owner verifies the sizes there.

## Day view

![The day view: the usage of the day and the windows, for Claude and Copilot](/screenshots/day-light.png){.screenshot .light-only}

![The day view: the usage of the day and the windows, for Claude and Copilot](/screenshots/day-dark.png){.screenshot .dark-only}

The day view shows, per provider, the usage of the local day written out as "usage of budget" (a day without a budget says "No budget for this day.", a day without data says why) and below it the windows exactly as the tray card shows them: per window the label and the percentage, the progress bar, the "Today" and "Month (projected)" rows where the window has a budget, `x of y used` and `resets …`. The windows are the current ones of `tamias today`, whatever day is open; if that report cannot be read, the day is shown without them. A stale provider says since when. The view reloads on every change of the store, like the calendar.

## Month chart

Below the calendar grid, each provider with a budget has a month chart: the cumulative usage of the month as a line against the cumulative budget, a straight line from 0 to the monthly budget at the end of the month. A day without data adds nothing and leaves a gap in the line. For the current month a dashed line continues from the last usage point to the projected month end (the month projection described under [Calendar](#calendar)). The usage line and the projection take the colour of the projection; a past month shows no projection.

## Year view

"Year" in the calendar header opens the year view at `/year/<year>`. It has one panel per provider with a bar for each month, January to December. A bar's height is the month's usage against its monthly budget; the budget mark sits at 100 % and a bar is full at 150 %. The bar's colour follows the same limits as the calendar (green below 80 %, yellow up to and including 100 %, red above), months without data are empty, and the percentage and the month name are written under the bar. A click on a bar opens that month in the calendar. Previous year, "This year" and Next year move between years; "Calendar" returns to the month. The view is read through the `year` command of the app.

## Tray

The tray icon shows the worst colour of the binding windows of the set-up providers. Its summary is the line of `tamias today --short` (for example `C 62%`), shown as the tooltip (Windows and macOS) and as the first, disabled menu item (Linux shows no tooltips).

| Icon | When |
| --- | --- |
| Hollow gauge outline | no provider is set up |
| Green, yellow, red gauge | the worst binding window is green, yellow or red |
| Grey gauge | a set-up provider is stale or has no data, or the last read failed |

Grey wins over red: a stale provider's numbers may be outdated.

- Hovering the icon (Windows and macOS) shows a small usage card next to it: per provider the binding window, its usage and the reset. A window with a daily budget also shows "Today": today's usage in percent of today's budget, with its own bar in the budget colour. It hides when the pointer leaves the icon and the card, on Escape and when it loses focus. Its "Open tamias" link opens the window.
- Per binding window (Claude 7-day, Copilot monthly) with a budget, a "Month (projected)" row: the same projection as in the calendar (month usage so far / elapsed days x days in the month) as a percentage of the monthly budget, with a bar coloured by the projection itself (green below 80 %, yellow up to and including 100 %, red above; grey while the provider is stale). Windows without a budget (informational) have no such row.
- A left click on the icon (Windows and macOS) opens the window on the calendar and hides the card.
- A right click opens the menu: the summary, "Open tamias", "Show usage" and "Quit". "Open tamias" shows and focuses the window on the calendar. "Show usage" opens the card; on Linux, which sends no hover or click events, it is the only way to open the card, and "Open tamias" the only way to open the window.
- Closing the window hides it to the tray; the app keeps running and collection is not affected.
- "Quit" in the tray menu exits the app. On macOS the dock icon is hidden while the window is hidden.

If the tray icon cannot be created (for example on Linux without an AppIndicator library), the window is shown even when the app was started hidden, and closing the window closes the app, because there would be no other way to open it again.

## Unavailable providers

The app shows the same document as `tamias today --json`. A provider that is `unavailable` has no windows, no calendar chips and a grey tray (when all are unavailable). For Copilot that is: `copilot = false` in `config.toml` (old data is ignored), `auto` without a `gh` executable (not set up), and a seat without a countable monthly quota (`no premium quota` for a free seat, `unlimited`), as recorded by the last `tamias collect`. An error recorded after that makes it `stale` instead. With `gh` installed but logged out, Copilot is `stale` (never collected), not unavailable. See [Copilot](./installation.md#copilot).

## Updates

The status bar has "Check for updates", "Update to vX.Y.Z" and an "Auto-update" switch. The app asks `https://github.com/GravionLabs/tamias/releases/latest/download/latest.json` (written by the release workflow) for a newer version. With the switch on (the default, remembered across restarts) it checks once at start; it never installs without you: when a newer release exists the button reads "Update to vX.Y.Z", and clicking it downloads the bundle, checks its minisign signature against the public key in `tauri.conf.json`, installs it and restarts the app. Turn the switch off to check only with "Check for updates".

A failure (offline, GitHub unreachable, a signature that does not match) shows a message in the status bar ("Update check failed: ..." or "Update failed: ...") and changes nothing: the app keeps running and you can check again. The update signature protects the download; it is not the operating system's code signature.

Because the app is not signed or notarised, an updated macOS app is quarantined again: after every update run `xattr -cr /Applications/tamias.app` once (and start it again if macOS refuses). The maintainers sign the bundles with the secret `TAURI_SIGNING_PRIVATE_KEY` of the repository (empty password); the matching public key is in `tauri.conf.json`.

## Start at login

Settings has "Start tamias at login". The setting is the state of the operating system's autostart entry, so it is off until you turn it on. The entry starts the app with `--hidden`: only the tray icon appears, and the window opens with "Open tamias" in the tray menu, or on Windows and macOS with a left click on the icon (or the card's "Open tamias" link on hover).

Removing the app does not remove the autostart entry; turn it off in Settings first. In a dev build (`pnpm tauri dev`) the switch is disabled with a hint, because a dev executable cannot run on its own after a reboot.

## The CLI is still the collector

The app reads the store that `tamias collect` writes and never polls a source itself. Scripts and other consumers read the same numbers from `tamias today --json` ([Consumers](./consumers.md)).

## Development

This section is for contributors who build the app from the repository. The app lives in `app/` (Angular, with the Tauri shell in `app/src-tauri/`).

`externalBin` in `app/src-tauri/tauri.conf.json` needs the CLI as `app/src-tauri/binaries/tamias-<target-triple>[.exe]` (on Windows also `tamiasw-<target-triple>.exe`), and the Tauri build script needs it for every `cargo build`, `clippy` and `test` of the app crate. Build it first, in `app/`:

```sh
cd app
pnpm sidecar    # debug build of the CLI for the host, copied to src-tauri/binaries/
pnpm tauri dev  # then the app; rerun pnpm sidecar after changing the CLI
```

`pnpm tauri dev` applies no CSP (`devCsp` is null) and builds the frontend without optimization, so CSP problems only show in a release build. Check UI changes with `pnpm tauri build --no-bundle` and run `src-tauri/../target/release/tamias-app`. The production build must not inline critical CSS (`inlineCritical: false` in `app/angular.json`): Tauri would add a hash for the inline `<style>` to `style-src`, which makes browsers ignore `'unsafe-inline'` and block the component styles. `app/src/csp.spec.ts` guards this.

For a local `cargo clippy` or `cargo test -p tamias-app`, run `pnpm sidecar` before. The frontend tests are `pnpm test` and `pnpm check` in `app/`.
