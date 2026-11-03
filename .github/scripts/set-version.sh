#!/usr/bin/env bash
# Writes the released version into the Cargo workspace so a build of the release tag reports it (`tamias --version`).
# Usage: VERSION=1.2.3 .github/scripts/set-version.sh   (or: .github/scripts/set-version.sh 1.2.3)
# Sets `[workspace.package] version` and the `tamias-core` version in `[workspace.dependencies]` of the root
# Cargo.toml, then updates the workspace members in Cargo.lock (`cargo update --workspace`, offline first).
# Run by versioning/gh-release (input version-command) of GravionLabs/ci from the repository root.
set -euo pipefail

VERSION="${1:-${VERSION:-}}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "unexpected version: '$VERSION' (expected x.y.z)" >&2; exit 1; }

manifest=Cargo.toml
[ -f "$manifest" ] || { echo "$manifest not found; run from the repository root" >&2; exit 1; }

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
awk -v ver="$VERSION" '
  /^\[/ { section = $0 }
  section == "[workspace.package]" && /^version[[:space:]]*=/ { sub(/"[^"]*"/, "\"" ver "\""); pkg++ }
  section == "[workspace.dependencies]" && /^tamias-core[[:space:]]*=/ {
    if (sub(/version[[:space:]]*=[[:space:]]*"[^"]*"/, "version = \"" ver "\"")) dep++
  }
  { print }
  END {
    if (pkg != 1) { print "no single version in [workspace.package]" > "/dev/stderr"; exit 1 }
    if (dep != 1) { print "no tamias-core version in [workspace.dependencies]" > "/dev/stderr"; exit 1 }
  }
' "$manifest" > "$tmp"
cat "$tmp" > "$manifest"

cargo update --workspace --offline 2>/dev/null || cargo update --workspace

echo "workspace version set to $VERSION"
