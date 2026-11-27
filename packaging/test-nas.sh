#!/usr/bin/env bash
# Verify and execute the packaged CLI on a native Linux runner.
set -euo pipefail
target=${1:-}
case "$target" in
  x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ;;
  *) echo 'Expected an x86_64 or aarch64 Linux musl target.' >&2; exit 1 ;;
esac
version=$(bash packaging/version.sh)
archive="${ICATALOG_RELEASE_OUTPUT_DIR:-target/release-assets}/icatalog-$version-$target.tar.gz"
while IFS= read -r entry; do
  case "$entry" in
    icatalog|licenses/|licenses/craft-libs/|licenses/craft-libs/*|licenses/ui-fonts/|licenses/ui-fonts/*) ;;
    *) echo 'NAS archive must contain only the CLI and dependency notices.' >&2; exit 1 ;;
  esac
  case "/$entry/" in
    */../*|*/./*) echo 'Unsafe NAS archive path.' >&2; exit 1 ;;
  esac
done < <(tar -tzf "$archive")
mkdir -p target
scratch=$(mktemp -d target/nas-check.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
tar -xzf "$archive" -C "$scratch"
if [[ -d licenses/craft-libs ]]; then
  diff -r licenses/craft-libs "$scratch/licenses/craft-libs"
fi
if [[ -d licenses/ui-fonts ]]; then
  diff -r licenses/ui-fonts "$scratch/licenses/ui-fonts"
fi
bash packaging/check-static-elf.sh "$target" "$scratch/icatalog"
test "$("$scratch/icatalog" --version)" = "icatalog $version"
"$scratch/icatalog" init "$scratch/smoke.sqlite"
"$scratch/icatalog" open "$scratch/smoke.sqlite"
"$scratch/icatalog" status "$scratch/smoke.sqlite"
