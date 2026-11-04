# Desktop app

The desktop app shows the same numbers as `tamias today`: a month calendar with each day coloured by the daily budget, a day view with the usage over the day, and a tray icon with the traffic light. It only reads. Collection stays the scheduled `tamias collect` of the CLI, which the app can set up for you ([Installation](./installation.md#scheduled-collection)).

## Download

Every release on the [GitHub releases page](https://github.com/GravionLabs/tamias/releases) has the app bundles next to the CLI archives, named `tamias-app-<version>-<target>.<ext>`, and they are listed in `SHA256SUMS`:

| Platform | Bundle |
| --- | --- |
| macOS, Apple silicon | `tamias-app-<version>-aarch64-apple-darwin.dmg` |
| macOS, Intel | `tamias-app-<version>-x86_64-apple-darwin.dmg` |
| Windows, x86_64 | `tamias-app-<version>-x86_64-pc-windows-msvc.msi` |
| Linux, x86_64 | `tamias-app-<version>-x86_64-unknown-linux-gnu.AppImage` or `.deb` |

Check the checksum as described in [Installation](./installation.md#download). Each bundle contains the `tamias` CLI of the same version (on Windows also `tamiasw.exe`), so the app does not need a separate CLI install.

The bundles are not signed or notarised (v1 ships unsigned, see the roadmap). Each system warns on the first start:

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
- Linux: the AppImage is mounted at a new path on every start, so the app first copies the CLI to `~/.local/bin/tamias` (replacing an older copy) and the systemd timer runs that copy. Put `~/.local/bin` on your `PATH` if you want to run `tamias` yourself.

"Not now" hides the prompt. Settings shows the collection status at any time, with "Set up" and "Remove". A collector you registered with a CLI from the archives keeps working: the app shows it as set up and does not replace it.

Every call of the CLI is limited to 10 seconds; a call that fails or does not answer is shown as an error with its message.

"Remove" in Settings (or `tamias service uninstall`) removes the registration; it keeps the data and the log. Remove the registration before you delete the app, otherwise the scheduler keeps running a missing executable.

## Calendar

The calendar opens on the current month. Previous, Today and Next move between months. Claude (`C`) and Copilot (`CP`) are shown side by side in each day's cell. A chip's colour is the colour of that day against the daily budget:

| Colour | Meaning |
| --- | --- |
| Green | below 80 % of the daily budget |
| Yellow | from 80 % up to and including 100 % |
| Red | above 100 % |
| Grey | today only: the provider is stale |
| Neutral | no data for that day, or the provider is not available |

A provider is stale when its last successful read is more than 15 minutes old, or it has never succeeded. On today's cell the grey chip says "stale since 09:00" (the time of the last successful collect), or "stale, never collected". Today's cell is only grey while the provider is stale; the calendar does not show old numbers as current.

The calendar refreshes when the collector writes new snapshots, so the app needs no reload. The first data appears with the next collect, within 5 minutes.

Click a day to open its day view. The "Today overview" link shows today's report of every provider.

## Day view

The day view shows, per provider, the usage over the local day as a curve against the day's budget line. Next to the chart the usage of the day is written out as "usage of budget"; a day without a budget says "No budget for this day.". The view reloads on every change of the store, like the calendar.

## Tray

The tray icon shows the worst colour of the binding windows of the set-up providers. Its summary is the line of `tamias today --short` (for example `C 62% · CP 41%`), shown as the tooltip (Windows and macOS) and as the first, disabled menu item (Linux shows no tooltips).

| Icon | When |
| --- | --- |
| Hollow ring | no provider is set up |
| Green, yellow, red disc | the worst binding window is green, yellow or red |
| Grey disc | a set-up provider is stale or has no data, or the last read failed |

Grey wins over red: a stale provider's numbers may be outdated.

- A left click on the icon (Windows and macOS), or "Open tamias" in its menu, shows and focuses the window.
- Closing the window hides it to the tray; the app keeps running and collection is not affected.
- "Quit" in the tray menu exits the app. On macOS the dock icon is hidden while the window is hidden.

If the tray icon cannot be created (for example on Linux without an AppIndicator library), the window is shown even when the app was started hidden, and closing the window closes the app, because there would be no other way to open it again.

## Start at login

Settings has "Start tamias at login". The setting is the state of the operating system's autostart entry, so it is off until you turn it on. The entry starts the app with `--hidden`: only the tray icon appears, and the window opens with "Open tamias" in the tray menu or a left click on the icon.

Removing the app does not remove the autostart entry; turn it off in Settings first.

## The CLI is still the collector

The app reads the store that `tamias collect` writes and never polls a source itself. Scripts and other consumers read the same numbers from `tamias today --json` ([Consumers](./consumers.md)).

## Development

This section is for contributors who build the app from the repository. The app lives in `app/` (Angular, with the Tauri shell in `app/src-tauri/`).

`externalBin` in `app/src-tauri/tauri.conf.json` needs the CLI as `app/src-tauri/binaries/tamias-<target-triple>` (on Windows also `tamiasw-<target-triple>.exe`), and the Tauri build script needs it for every `cargo build`, `clippy` and `test` of the app crate. Build it first, in `app/`:

```sh
cd app
pnpm sidecar    # debug build of the CLI for the host, copied to src-tauri/binaries/
pnpm tauri dev  # then the app; rerun pnpm sidecar after changing the CLI
```

For a local `cargo clippy` or `cargo test -p tamias-app`, run `pnpm sidecar` before. The frontend tests are `pnpm test` and `pnpm check` in `app/`.
