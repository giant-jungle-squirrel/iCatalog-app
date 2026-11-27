#!/usr/bin/env bash
# Run from the application source root, including historical release recovery.
set -euo pipefail
if [[ $# != 1 || -z "$1" ]]; then
  echo 'Usage: copy-notices.sh <destination-directory>' >&2
  exit 1
fi
if [[ ! -d licenses/craft-libs ]]; then
  if grep -Eq '^name = "craft-(ui|raw|pdf)"$' Cargo.lock; then
    echo 'Missing licenses/craft-libs notices required by this application source.' >&2
    exit 1
  fi
else
  for required in README.md SOURCE.json LICENSE-MIT LICENSE-APACHE NOTICE \
    pdf-backend/hayro-interpret/assets/LICENSE_FOXIT pdf-backend/hayro-cmap/LICENSE.txt; do
    if [[ ! -s "licenses/craft-libs/$required" ]]; then
      echo "Missing dependency notice: licenses/craft-libs/$required" >&2
      exit 1
    fi
  done
  mkdir -p "$1"
  cp -R licenses/craft-libs "$1/"
fi
# Recovery builds use the historical application's assets and notices. Releases
# predating the bundled UI fonts do not require the new notice bundle.
if [[ -d licenses/ui-fonts ]]; then
  for required in README.md OFL-Inter.txt OFL-JetBrainsMono.txt; do
    if [[ ! -s "licenses/ui-fonts/$required" ]]; then
      echo "Missing UI font notice: licenses/ui-fonts/$required" >&2
      exit 1
    fi
  done
  mkdir -p "$1"
  cp -R licenses/ui-fonts "$1/"
elif [[ -d crates/icatalog-ui/assets/fonts ]]; then
  echo 'Missing licenses/ui-fonts notices required by this application source.' >&2
  exit 1
fi
