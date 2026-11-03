#!/usr/bin/env bash
# Packs the release binaries of one target into tamias-<version>-<target>.tar.gz (Windows targets: .zip).
# Usage: .github/scripts/package-release.sh <version> <target> <bin-dir> <out-dir>
#   <bin-dir>  directory with the built binaries (target/<target>/release)
#   <out-dir>  directory the archive is written to (created if missing)
# The archive holds one top-level directory tamias-<version>-<target>/ with tamias (tamias.exe), on Windows also
# tamiasw.exe when it was built (#30), and LICENSE and README.md of the repository root (the current directory).
# Prints the path of the archive. Run by .github/workflows/release-binaries.yml from the repository root.
set -euo pipefail

[ "$#" -eq 4 ] || { echo "usage: $0 <version> <target> <bin-dir> <out-dir>" >&2; exit 2; }
version="$1" target="$2" bin_dir="$3" out_dir="$4"

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "unexpected version: '$version'" >&2; exit 1; }
[ -n "$target" ] || { echo "empty target" >&2; exit 1; }
for doc in LICENSE README.md; do
  [ -f "$doc" ] || { echo "$doc not found; run from the repository root" >&2; exit 1; }
done

case "$target" in
  *-windows-*) exe=.exe format=zip ;;
  *) exe='' format=tar.gz ;;
esac

[ -f "$bin_dir/tamias$exe" ] || { echo "$bin_dir/tamias$exe not found" >&2; exit 1; }

name="tamias-$version-$target"
mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/$name"

cp "$bin_dir/tamias$exe" "$stage/$name/"
# The windowless GUI bin of #30; optional until it exists.
if [ -n "$exe" ] && [ -f "$bin_dir/tamiasw$exe" ]; then
  cp "$bin_dir/tamiasw$exe" "$stage/$name/"
fi
cp LICENSE README.md "$stage/$name/"

archive="$out_dir/$name.$format"
rm -f "$archive"
if [ "$format" = zip ]; then
  # windows runners ship 7z but not always zip
  if command -v zip >/dev/null 2>&1; then
    (cd "$stage" && zip -qr "$archive" "$name")
  else
    (cd "$stage" && 7z a -tzip -bso0 -bsp0 "$archive" "$name")
  fi
else
  tar -C "$stage" -czf "$archive" "$name"
fi

echo "$archive"
