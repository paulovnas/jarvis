#!/bin/sh
set -eu

if [ "$#" -eq 0 ] || [ -z "${APPLE_SIGNING_IDENTITY:-}" ] || [ "${APPLE_SIGNING_IDENTITY:-}" = "-" ] || [ -z "${JARVIS_SIGNING_IDENTIFIER:-}" ]; then
  echo "Assinatura do Jarvis não configurada. Inicie com bun run tauri dev." >&2
  exit 1
fi

jarvis_executable=$1
shift
jarvis_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

# CEF's pinned CLI owns the complete app bundle and its nested signatures. Never
# repackage that executable into the small notification-only development bundle.
jarvis_binary_dir=$(CDPATH= cd -- "$(dirname -- "$jarvis_executable")" && pwd)
jarvis_cef_bundle=
case "$jarvis_binary_dir" in
  *.app/Contents/MacOS)
    jarvis_candidate=${jarvis_binary_dir%/Contents/MacOS}
    jarvis_main_name=${jarvis_executable##*/}
    # Shared Core runtimes ship the per-app helpers while loading the framework
    # externally; an embedded distribution ships both. Preserve either bundle.
    if [ -d "$jarvis_candidate/Contents/Frameworks/Chromium Embedded Framework.framework" ] || [ -x "$jarvis_candidate/Contents/Frameworks/$jarvis_main_name Helper.app/Contents/MacOS/$jarvis_main_name Helper" ]; then
      jarvis_cef_bundle=$jarvis_candidate
    fi
    ;;
esac
if [ -n "$jarvis_cef_bundle" ]; then
  /usr/bin/codesign --verify --deep --strict "$jarvis_cef_bundle"
  exec "$jarvis_executable" "$@"
fi
if [ "${JARVIS_CEF_RUNTIME:-}" = "1" ]; then
  echo "O Chromium precisa do bundle completo com framework e helpers. Inicie com bun run tauri dev usando o CLI fixado do Jarvis." >&2
  exit 1
fi

# Sign before exec, so the Keychain sees the same certificate and identifier after each rebuild.
# exec preserves Cargo/Tauri's process lifecycle, signals, exit status and application arguments.
/usr/bin/codesign --force --sign "$APPLE_SIGNING_IDENTITY" --identifier "$JARVIS_SIGNING_IDENTIFIER" --entitlements "$jarvis_root/src-tauri/Entitlements.plist" --timestamp=none "$jarvis_executable"
/usr/bin/codesign --verify --strict "$jarvis_executable"

if [ "${JARVIS_DEV_APP_BUNDLE:-}" = "1" ]; then
  # UserNotifications requires an actual .app, even for a signed dev executable.
  # Keep Cargo's exec lifecycle and Vite HMR while providing the native identity.
  jarvis_target=$(CDPATH= cd -- "$(dirname -- "$jarvis_executable")" && pwd)
  jarvis_bundle="$jarvis_target/jarvis-dev/Jarvis.app"
  /bin/mkdir -p "$jarvis_bundle/Contents/MacOS" "$jarvis_bundle/Contents/Resources"
  /bin/cp -f "$jarvis_executable" "$jarvis_bundle/Contents/MacOS/Jarvis"
  /bin/cp -f "$jarvis_root/src-tauri/icons/icon.icns" "$jarvis_bundle/Contents/Resources/icon.icns"
  /bin/cp -f "$jarvis_root/scripts/macos-dev-Info.plist" "$jarvis_bundle/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $JARVIS_SIGNING_IDENTIFIER" "$jarvis_bundle/Contents/Info.plist"
  jarvis_version=$(/usr/bin/plutil -extract version raw -o - "$jarvis_root/src-tauri/tauri.conf.json")
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $jarvis_version" "$jarvis_bundle/Contents/Info.plist"
  /usr/bin/codesign --force --sign "$APPLE_SIGNING_IDENTITY" --identifier "$JARVIS_SIGNING_IDENTIFIER" --entitlements "$jarvis_root/src-tauri/Entitlements.plist" --timestamp=none "$jarvis_bundle"
  /usr/bin/codesign --verify --strict "$jarvis_bundle"
  jarvis_executable="$jarvis_bundle/Contents/MacOS/Jarvis"
fi
exec "$jarvis_executable" "$@"
