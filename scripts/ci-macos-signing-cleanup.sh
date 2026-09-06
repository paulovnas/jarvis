#!/bin/bash
set -euo pipefail
: "${RUNNER_TEMP:?}"
signing_dir="$RUNNER_TEMP/jarvis-signing"
if [[ -f "$signing_dir/build.keychain-db" ]]; then
  security delete-keychain "$signing_dir/build.keychain-db"
fi
rm -rf "$signing_dir"
