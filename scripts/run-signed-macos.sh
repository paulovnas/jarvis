#!/bin/sh
set -eu

if [ "$#" -eq 0 ] || [ -z "${APPLE_SIGNING_IDENTITY:-}" ] || [ "${APPLE_SIGNING_IDENTITY:-}" = "-" ] || [ -z "${JARVIS_SIGNING_IDENTIFIER:-}" ]; then
  echo "Assinatura do Jarvis não configurada. Inicie com bun run tauri dev." >&2
  exit 1
fi

jarvis_executable=$1
shift

# Sign before exec, so the Keychain sees the same certificate and identifier after each rebuild.
# exec preserves Cargo/Tauri's process lifecycle, signals, exit status and application arguments.
/usr/bin/codesign --force --sign "$APPLE_SIGNING_IDENTITY" --identifier "$JARVIS_SIGNING_IDENTIFIER" --timestamp=none "$jarvis_executable"
/usr/bin/codesign --verify --strict "$jarvis_executable"
exec "$jarvis_executable" "$@"
