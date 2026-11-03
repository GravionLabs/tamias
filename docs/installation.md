# Installation

## Download

Every release on the [GitHub releases page](https://github.com/GravionLabs/tamias/releases) has one archive per platform and a `SHA256SUMS` file with the checksum of every archive. The archive is named `tamias-<version>-<target>` and holds one directory with `tamias` (Windows: `tamias.exe`), `LICENSE` and `README.md`.

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

To poll every 5 minutes without running `collect` by hand, see [Scheduled collection](#scheduled-collection).

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

### The collect log

Every scheduled run appends one line per provider to `logs/collect.log` in the data directory: time, provider, `ok` or `error`, a short message. It never contains a token. Above 1 MiB the file is rotated to `collect.log.1` (one generation is kept). The scheduler's own stdout and stderr are discarded; this file is the log.

| Platform | Data directory | Collect log |
|----------|----------------|-------------|
| macOS | `~/Library/Application Support/tamias` | `~/Library/Application Support/tamias/logs/collect.log` |
| Linux | `$XDG_DATA_HOME/tamias` (default `~/.local/share/tamias`) | `~/.local/share/tamias/logs/collect.log` |
| Windows | `%APPDATA%\tamias` | `%APPDATA%\tamias\logs\collect.log` |

`TAMIAS_DATA_DIR` overrides the data directory on every platform. Run `tamias service status` to print the actual log path.

### Check the last run

`tamias service status` prints the registration, the executable, the last run of each provider (from the store) and the log path. For the history, read the end of the log:

```text
2026-11-03T10:15:42Z claude ok stored 2 snapshots
2026-11-03T10:15:42Z copilot error unauthorized
```

An `error` line names a class of failure, for example an expired login; fix it and the next run recovers. A source that keeps failing shows as `stale` in `tamias today`.

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
