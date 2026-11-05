# Installation

## Download

Every release on the [GitHub releases page](https://github.com/GravionLabs/tamias/releases) has one archive per platform and a `SHA256SUMS` file with the checksum of every archive. The archive is named `tamias-<version>-<target>` and holds one directory with `tamias` (Windows: `tamias.exe`), `LICENSE` and `README.md`. The Windows archive also holds `tamiasw.exe`, which `tamias service install` needs next to `tamias.exe`.

| Platform | Archive |
| --- | --- |
| Linux, x86_64 | `tamias-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux, arm64 | `tamias-<version>-aarch64-unknown-linux-musl.tar.gz` |
| macOS, Apple silicon | `tamias-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `tamias-<version>-x86_64-apple-darwin.tar.gz` |
| Windows, x86_64 | `tamias-<version>-x86_64-pc-windows-msvc.zip` |

The Linux binaries are statically linked and need no installed libraries. The binaries are not signed, so macOS and Windows warn on the first start (see the platform sections).

Download the archive of your platform and `SHA256SUMS` into the same empty directory. Replace `<version>` below with the version you downloaded.

### Linux

```sh
sha256sum --ignore-missing -c SHA256SUMS
tar -xzf tamias-<version>-x86_64-unknown-linux-musl.tar.gz
install -m 755 tamias-<version>-x86_64-unknown-linux-musl/tamias ~/.local/bin/tamias
```

`~/.local/bin` must be on your `PATH`. The check prints `OK` for the archive you downloaded. Linux does not warn on the first start.

### macOS

```sh
shasum -a 256 --ignore-missing -c SHA256SUMS
tar -xzf tamias-<version>-aarch64-apple-darwin.tar.gz
mkdir -p ~/.local/bin
cp tamias-<version>-aarch64-apple-darwin/tamias ~/.local/bin/tamias
```

Put `~/.local/bin` on your `PATH` (for zsh: `export PATH="$HOME/.local/bin:$PATH"` in `~/.zshrc`). If your `shasum` has no `--ignore-missing`, compare `shasum -a 256 <archive>` with the line in `SHA256SUMS` by eye.

Gatekeeper blocks a downloaded, unsigned binary on the first start. Remove the quarantine flag once:

```sh
xattr -d com.apple.quarantine ~/.local/bin/tamias
```

Alternatively start `tamias` once, then open System Settings, Privacy & Security, and choose "Allow Anyway" for tamias.

### Windows

In PowerShell, in the download directory:

```powershell
$archive = "tamias-<version>-x86_64-pc-windows-msvc.zip"
(Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
Select-String -Path SHA256SUMS -Pattern $archive
```

The two hashes must be identical. Then unpack and put the directory on your `PATH`:

```powershell
Expand-Archive $archive -DestinationPath "$env:LOCALAPPDATA\tamias"
$dir = "$env:LOCALAPPDATA\tamias\tamias-<version>-x86_64-pc-windows-msvc"
[Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path', 'User'));$dir", "User")
```

Open a new terminal for the changed `PATH` to apply. Windows SmartScreen may warn about an unrecognized app on the first start: choose "More info", then "Run anyway".

### First start

```sh
tamias --version
tamias collect    # poll Claude and Copilot once and store the snapshots
tamias today      # today's usage against the daily budget
```

To poll every 5 minutes without running `collect` by hand, see [Scheduled collection](#scheduled-collection). To show the usage in the Claude Code status line, see [Claude Code status line](./statusline.md).

## Copilot

tamias polls GitHub Copilot through the GitHub CLI `gh`; it reads the token of `gh auth token` and sends it only to `api.github.com`.

1. Install [`gh`](https://cli.github.com/) and run `gh auth login` once.
2. Run `tamias collect`, then `tamias today`.

Until the first collect, `tamias today` shows Copilot as `stale, never collected` and exits with 3. After a successful collect it shows the monthly quota like Claude's windows.

Two optional settings in `config.toml`, which lives in the config directory:

| Platform | Config directory |
|----------|------------------|
| macOS | `~/Library/Application Support/tamias` |
| Linux | `$XDG_CONFIG_HOME/tamias` (default `~/.config/tamias`) |
| Windows | `%APPDATA%\tamias\config` |

`TAMIAS_CONFIG_DIR` overrides the config directory on every platform.

```toml
gh_path = "/opt/gh/bin/gh"   # top level: use this gh instead of searching for one

[providers]
copilot = "auto"             # "auto" (default), true or false
```

- `"auto"`: Copilot is polled when a `gh` executable can be found (`gh_path`, then `PATH` and the usual install locations). Without one it shows as `unavailable (not set up)`. Login is not checked up front: with `gh` installed but logged out (`gh auth token` fails), the run is logged as skipped in `logs/collect.log` and nothing is stored, so `tamias today` shows Copilot as `stale, never collected` until you run `gh auth login`.
- `true`: always poll; a missing or logged-out `gh` is an error (`stale` with the error), not `unavailable`.
- `false`: opt out. Copilot shows as `unavailable (disabled)` and is skipped.

A seat without a countable monthly quota is not an error. A free seat shows as `unavailable (no premium quota)`, a seat with unlimited premium requests as `unavailable (unlimited)`. Neither has a daily budget, and neither turns the exit code to 3.

## Configuration

All settings are optional and live in `config.toml` in the config directory; a missing file means the defaults. An invalid value or an unknown key is an error that names the key. Example with every key:

```toml
yellow_at = 0.8       # ratio of today's usage to today's budget from which a day is yellow
red_at = 1.0          # ratio above which a day is red
stale_after = "15m"   # age of the last successful collect after which data is stale
week_start = "monday" # first day of the app's calendar week

[providers]
claude = "auto"       # "auto" (on when credentials exist), true or false
```

`week_start` is `"monday"` (default), `"sunday"`, `"saturday"` or `"locale"` (the first day of the system locale). It only changes how the desktop app's month calendar is laid out, not how the CLI reports days or budgets.

## Scheduled collection

tamias reads Claude and Copilot usage by polling every 5 minutes. `tamias service` registers that poll with the scheduler of your operating system. It runs in your user account on all three platforms: no administrator rights, no `sudo`, no stored password.

```text
tamias service install     # register the collector (run again to replace it)
tamias service status      # registered or not, which exe, last run per provider, log path
tamias service uninstall   # remove the registration (exits 0 with a note when there is none)
```

`install` registers the executable you run it from, so install tamias where it is going to stay. Installing twice leaves exactly one registration. `status` warns when the registered executable no longer exists; run `install` again after moving or upgrading tamias to a new path.

| Platform | Registered as | Where |
|----------|---------------|-------|
| macOS | launchd agent `com.gravionlabs.tamias.collect`, every 300 s and once at load | `~/Library/LaunchAgents/com.gravionlabs.tamias.collect.plist` |
| Linux | systemd user timer `tamias-collect.timer` and service `tamias-collect.service` | `$XDG_CONFIG_HOME/systemd/user/` (default `~/.config/systemd/user/`) |
| Windows | Task Scheduler task `\GravionLabs\tamias-collect`, every 5 minutes | Task Scheduler, runs `tamiasw.exe` |

### From the desktop app

The desktop app only reads; collecting stays the scheduled `tamias collect`. Its bundle ships the `tamias` CLI of the same version (on Windows also `tamiasw.exe`), so the app can set up collection without a separate CLI install:

- On the first start without data and without a registered collector, the calendar asks "Collection is not set up — set up now?". Settings shows the status with "Set up" and "Remove" at any time.
- "Set up" runs `tamias service install` of the bundled CLI, so the scheduler runs the CLI inside the installed app: `tamias.app/Contents/MacOS/tamias` on macOS, `tamiasw.exe` in the install directory on Windows. On Linux the AppImage is mounted at a new path on every start, so the app first copies the CLI to `~/.local/bin/tamias` (replacing an older copy) and the systemd unit runs that copy.
- A collector you registered with a CLI from the archives keeps working: the app shows it as set up and does not replace it.
- Every call of the CLI is limited to 10 seconds; a call that fails or does not answer shows as an error with its message.
- Removing the app does not remove the collector; run "Remove" in the settings (or `tamias service uninstall`) first.

### The collect log

Every scheduled run appends one line per provider to `logs/collect.log` in the data directory: time, provider, `ok` or `error`, a short message. A fatal error (invalid config, unreadable data directory or store) is one line too, `error fatal: <reason>`. It is not stored as a provider result, so `tamias today` and `tamias service status` are unaffected; the config path is left out of it. It never contains a token. Above 1 MiB the file is rotated to `collect.log.1` (one generation is kept). The scheduler's own stdout and stderr are discarded; this file is the log.

| Platform | Data directory | Collect log |
|----------|----------------|-------------|
| macOS | `~/Library/Application Support/tamias` | `~/Library/Application Support/tamias/logs/collect.log` |
| Linux | `$XDG_DATA_HOME/tamias` (default `~/.local/share/tamias`) | `~/.local/share/tamias/logs/collect.log` |
| Windows | `%APPDATA%\tamias` | `%APPDATA%\tamias\logs\collect.log` |

`TAMIAS_DATA_DIR` overrides the data directory on every platform. Run `tamias service status` to print the actual log path.

### Check the last run

`tamias service status` prints the registration, the executable, the last run per provider (Claude and Copilot, from the store) and the log path. For the history, read the end of the log:

```text
2026-11-03T10:15:42Z claude ok stored 2 snapshots
2026-11-03T10:20:42Z claude error unauthorized
```

An `error` line names a class of failure, for example an expired login; fix it and the next run recovers. A source that keeps failing shows as `stale` in `tamias today`.

### `gh` not found under the service

The scheduler starts `tamias collect` with a minimal `PATH`, often without the directory of `gh` (tamias itself also looks in `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin` and `/snap/bin`, but not in places such as `~/.local/bin` or a custom prefix). `unavailable (not set up)` means `gh` was not found. If `tamias collect` works in your shell but Copilot shows `unavailable (not set up)` under the service, set `gh_path` in `config.toml` (config directory, see [Copilot](#copilot)) to the full path of `gh` (`which gh`, `Get-Command gh` on Windows). The service must also run as the user that ran `gh auth login`, because the login lives in that user's `gh` configuration or keyring. If Copilot shows `stale, never collected` instead, `gh` was found but is logged out for that user: run `gh auth login` as that user. Check `logs/collect.log` for the `copilot` lines.

### macOS

- Install: writes the plist and loads it with `launchctl bootstrap gui/<uid>`; a reinstall replaces the agent (`bootout`, write, `bootstrap`).
- Uninstall: `launchctl bootout` and removes the plist.
- Inspect: `launchctl print gui/$(id -u)/com.gravionlabs.tamias.collect`.
- After sleep: launchd runs a `StartInterval` job that was missed during sleep once on wake.

### Linux

- Requires a systemd user manager (`systemctl --user`). Without one, `install` fails with a message instead of leaving partial files.
- Install: writes `tamias-collect.service` (a oneshot running `<exe> collect`) and `tamias-collect.timer`, then enables and starts the timer. The timer fires 1 minute after the user manager starts, then 5 minutes after each run, with `Persistent=true`.
- Uninstall: disables and stops the timer and removes both unit files.
- Inspect: `systemctl --user list-timers tamias-collect.timer`, `journalctl --user -u tamias-collect.service`.
- After sleep: the timer is relative to the last run (`OnUnitActiveSec`), with `Persistent=true`. Not verified on a real machine (see the sleep checks below); if a run is missing after resume, the next one follows within the interval.
- To collect while you are logged out, enable lingering once: `loginctl enable-linger $USER`.

### Windows

- Needs `tamiasw.exe` in the same directory as `tamias.exe`; `install` refuses otherwise. `tamiasw.exe` is `tamias collect` without a console window, so nothing flashes every 5 minutes.
- Install: `schtasks /Create /XML` with a task for the current user, least privilege, interactive token; `/F` replaces an existing task.
- Uninstall: `schtasks /Delete` for the task, if it exists.
- Inspect: `schtasks /Query /TN \GravionLabs\tamias-collect` or the Task Scheduler app, folder GravionLabs.
- After sleep: the task has "run as soon as possible after a scheduled start is missed" and runs on battery; a run is limited to 2 minutes and never overlaps itself.

### Sleep checks

The manual checks for the three platforms (agent or timer fires every 5 minutes; a run missed during sleep happens on wake) are summarised honestly here. The tests of the three services cover the generated files and commands with fake runners; the real scheduler behaviour needs a real machine.

| Platform | Date | Result |
|----------|------|--------|
| macOS | not recorded | Planned in the PR [#47](https://github.com/GravionLabs/tamias/pull/47): sleep the Mac for more than 10 minutes, expect one log line shortly after waking. No result has been recorded in the PR or the issue. |
| Linux | not done | Pending in PR [#50](https://github.com/GravionLabs/tamias/pull/50) ("pending the owner's Linux machine"). |
| Windows | not done | Pending in PR [#54](https://github.com/GravionLabs/tamias/pull/54) ("Not yet run"): no console window, every 5 minutes, missed run after sleep. |

The "After sleep" lines above describe what the scheduler is configured to do, not a verified result. This table is updated when the checks are done.

### Remove everything

1. `tamias service uninstall` removes the registration. It never deletes data.
2. Delete the data directory (database, `logs/`) listed above if you want the history gone. On Linux and macOS: `rm -r "<data directory>"`; on Windows: `Remove-Item -Recurse "$env:APPDATA\tamias"`.
3. Delete the executables (`tamias`, and `tamiasw.exe` on Windows).

Do step 1 first: a registration pointing to a deleted executable fails every 5 minutes.
