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

if [ "${JARVIS_DEV_APP_BUNDLE:-}" = "1" ]; then
  # UserNotifications requires an actual .app, even for a signed dev executable.
  # Keep Cargo's exec lifecycle and Vite HMR while providing the native identity.
  jarvis_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
  jarvis_target=$(CDPATH= cd -- "$(dirname -- "$jarvis_executable")" && pwd)
  jarvis_bundle="$jarvis_target/jarvis-dev/Jarvis.app"
  /bin/mkdir -p "$jarvis_bundle/Contents/MacOS" "$jarvis_bundle/Contents/Resources"
  /bin/cp -f "$jarvis_executable" "$jarvis_bundle/Contents/MacOS/Jarvis"
  /bin/cp -f "$jarvis_root/src-tauri/icons/icon.icns" "$jarvis_bundle/Contents/Resources/icon.icns"
  /bin/cp -f "$jarvis_root/scripts/macos-dev-Info.plist" "$jarvis_bundle/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $JARVIS_SIGNING_IDENTIFIER" "$jarvis_bundle/Contents/Info.plist"
  jarvis_version=$(/usr/bin/plutil -extract version raw -o - "$jarvis_root/src-tauri/tauri.conf.json")
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $jarvis_version" "$jarvis_bundle/Contents/Info.plist"
  /usr/bin/codesign --force --sign "$APPLE_SIGNING_IDENTITY" --identifier "$JARVIS_SIGNING_IDENTIFIER" --timestamp=none "$jarvis_bundle"
  /usr/bin/codesign --verify --strict "$jarvis_bundle"
  jarvis_executable="$jarvis_bundle/Contents/MacOS/Jarvis"
fi
exec "$jarvis_executable" "$@"
