#!/bin/bash
set -euo pipefail
# Secrets are provided only to this step; never enable shell tracing.
: "${RUNNER_TEMP:?}"
: "${GITHUB_ENV:?}"
: "${APPLE_CERTIFICATE:?}"
: "${APPLE_CERTIFICATE_PASSWORD:?}"
: "${APPLE_SIGNING_IDENTITY:?}"
: "${TAURI_SIGNING_PRIVATE_KEY:?}"

signing_dir="$RUNNER_TEMP/jarvis-signing"
mkdir -p "$signing_dir"
chmod 700 "$signing_dir"
keychain="$signing_dir/build.keychain-db"
keychain_password="$(openssl rand -hex 32)"
export JARVIS_CI_SIGNING_DIR="$signing_dir"
python3 - <<'PY'
import base64, os
directory = os.environ["JARVIS_CI_SIGNING_DIR"]
for name, content in [
    ("identity.p12", base64.b64decode(os.environ["APPLE_CERTIFICATE"], validate=True)),
    ("updater.key", os.environ["TAURI_SIGNING_PRIVATE_KEY"].encode()),
]:
    with open(os.path.join(directory, name), "wb") as output:
        output.write(content)
    os.chmod(os.path.join(directory, name), 0o600)
PY

security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$signing_dir/identity.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain" "$HOME/Library/Keychains/login.keychain-db"
# This lookup requires a valid, trusted certificate with its corresponding private key.
security find-identity -v -p codesigning "$keychain" | /usr/bin/grep -F "$APPLE_SIGNING_IDENTITY" >/dev/null
rm -f "$signing_dir/identity.p12"
printf 'TAURI_SIGNING_PRIVATE_KEY=%s\n' "$signing_dir/updater.key" >> "$GITHUB_ENV"
