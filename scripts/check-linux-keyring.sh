#!/usr/bin/env bash
# A private bus and data directory keep synthetic credentials out of the user's wallet.
set -euo pipefail
cd "$(dirname "$0")/.."
test "$(uname -s)" = Linux
for program in dbus-run-session gnome-keyring-daemon gdbus; do
  command -v "$program" >/dev/null
done
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --no-run
directory=$(mktemp -d)
trap 'rm -rf "$directory"' EXIT
cat > "$directory/bus.conf" <<'XML'
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
XML
dbus-run-session --config-file "$directory/bus.conf" -- bash -euo pipefail -c '
  directory=$1
  export XDG_DATA_HOME="$directory/data"
  export XDG_CONFIG_HOME="$directory/config"
  export XDG_RUNTIME_DIR="$directory/runtime"
  mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_RUNTIME_DIR" "$directory/control"
  chmod 700 "$XDG_RUNTIME_DIR"
  unset GNOME_KEYRING_CONTROL GNOME_KEYRING_PID
  cargo test --locked --manifest-path src-tauri/Cargo.toml --lib linux_secret_service_unavailable -- --ignored --test-threads=1
  start_keyring() {
    printf "%s" "jarvis-synthetic-test-password" | gnome-keyring-daemon --foreground --unlock --components=secrets --control-directory="$directory/control" >"$directory/keyring.log" 2>&1 &
    keyring_pid=$!
    for attempt in {1..50}; do
      if gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then return; fi
      sleep 0.1
    done
    cat "$directory/keyring.log"; return 1
  }
  start_keyring
  trap '\''kill "$keyring_pid" 2>/dev/null || true; wait "$keyring_pid" 2>/dev/null || true'\'' EXIT
  cargo test --locked --manifest-path src-tauri/Cargo.toml --lib linux_secret_service_round_trip -- --ignored --test-threads=1
  JARVIS_KEYRING_TEST_PHASE=store cargo test --locked --manifest-path src-tauri/Cargo.toml --lib linux_secret_service_persistence -- --ignored --test-threads=1
  gdbus call --session --dest org.freedesktop.secrets --object-path /org/freedesktop/secrets --method org.freedesktop.Secret.Service.Lock "[objectpath '\''/org/freedesktop/secrets/collection/login'\'']" >/dev/null
  JARVIS_KEYRING_TEST_PHASE=locked cargo test --locked --manifest-path src-tauri/Cargo.toml --lib linux_secret_service_persistence -- --ignored --test-threads=1
  kill "$keyring_pid"; wait "$keyring_pid" || true
  start_keyring
  JARVIS_KEYRING_TEST_PHASE=reopened cargo test --locked --manifest-path src-tauri/Cargo.toml --lib linux_secret_service_persistence -- --ignored --test-threads=1
' bash "$directory"
