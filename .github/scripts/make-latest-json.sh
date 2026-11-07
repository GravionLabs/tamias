#!/usr/bin/env bash
# Writes the latest.json of the Tauri updater (#163) to stdout.
# Usage: make-latest-json.sh <version> <tag> <owner/repo> <dir>
# <dir> holds the signed updater artifacts named tamias-app-<version>-<target>.<ext> with their .sig files:
# .app.tar.gz on macOS, .msi on Windows (.exe, the NSIS installer, for a release candidate x.y.z-rc.n), .AppImage on
# Linux.
set -euo pipefail

version="$1" tag="$2" repo="$3" dir="$4"
base="https://github.com/$repo/releases/download/$tag"

platforms='{}'
add() { # <updater platform> <file>
  local file="$dir/$2"
  [ -f "$file" ] && [ -f "$file.sig" ] || { echo "make-latest-json: $2 or $2.sig missing" >&2; exit 1; }
  platforms="$(jq --arg p "$1" --arg url "$base/$2" --rawfile sig "$file.sig" \
    '. + {($p): {signature: ($sig | rtrimstr("\n")), url: $url}}' <<<"$platforms")"
}

add darwin-aarch64 "tamias-app-$version-aarch64-apple-darwin.app.tar.gz"
add darwin-x86_64 "tamias-app-$version-x86_64-apple-darwin.app.tar.gz"
win_ext=msi
[[ "$version" == *-* ]] && win_ext=exe # WiX needs a numeric version, so a release candidate bundles NSIS
add windows-x86_64 "tamias-app-$version-x86_64-pc-windows-msvc.$win_ext"
add linux-x86_64 "tamias-app-$version-x86_64-unknown-linux-gnu.AppImage"

jq -n --arg version "$version" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson platforms "$platforms" \
  '{version: $version, notes: "", pub_date: $date, platforms: $platforms}'
