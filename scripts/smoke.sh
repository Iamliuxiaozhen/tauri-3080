#!/usr/bin/env bash
#
# Local smoke test.
#
# Builds the frontend and the binary, starts a throw-away service on port 3080,
# boots the app on a virtual display and asserts the JSON report.
#
# Usage:
#   scripts/smoke.sh            # full run
#   scripts/smoke.sh --keep     # keep the report and the config directory
#
# SMOKE_PORT (default 3080) picks the port the stand-in service listens on. When
# that port is already taken and CI is not set, a free port is used instead, so
# a developer running their own service on 3080 can still run the smoke test.
#
# Requirements: xvfb-run, dbus-run-session, python3, a built Tauri toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

KEEP=0
if [ "${1:-}" = "--keep" ]; then
  KEEP=1
fi

PORT="${SMOKE_PORT:-3080}"

port_in_use() {
  python3 - "$1" <<'PY'
import socket
import sys

sock = socket.socket()
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    sock.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(0)
finally:
    sock.close()
sys.exit(1)
PY
}

if port_in_use "$PORT"; then
  if [ "${CI:-}" = "true" ]; then
    echo "error: port $PORT is already in use" >&2
    exit 1
  fi
  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
  echo "note: port ${SMOKE_PORT:-3080} is busy, using $PORT for this run"
fi
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/tauri-3080-smoke.XXXXXX")"
CONFIG_DIR="$WORKDIR/config"
REPORT="$WORKDIR/report.json"
# Honour CARGO_TARGET_DIR so the script also works with an out-of-tree target
# directory (as the CI cache and `cargo --target-dir` users have).
TARGET_DIR="${CARGO_TARGET_DIR:-src-tauri/target}/debug"
BINARY="$TARGET_DIR/tauri-3080"
SERVER_PID=""

cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [ "$KEEP" -eq 0 ]; then
    rm -rf "$WORKDIR"
  else
    echo "kept the smoke workspace at $WORKDIR"
  fi
}
trap cleanup EXIT

echo "==> building the frontend"
npm run build >/dev/null

echo "==> building the binary (embedded assets)"
cargo build --manifest-path src-tauri/Cargo.toml --features custom-protocol

echo "==> starting a stand-in service on 127.0.0.1:$PORT"
mkdir -p "$WORKDIR/www"
cat >"$WORKDIR/www/index.html" <<'HTML'
<!doctype html>
<html lang="en">
  <head><meta charset="utf-8" /><title>stand-in service</title></head>
  <body><h1>127.0.0.1:3080</h1></body>
</html>
HTML
(cd "$WORKDIR/www" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >"$WORKDIR/http.log" 2>&1) &
SERVER_PID=$!

for _ in $(seq 1 40); do
  if curl -fsS -o /dev/null "http://127.0.0.1:$PORT/"; then
    break
  fi
  sleep 0.25
done
if ! curl -fsS -o /dev/null "http://127.0.0.1:$PORT/"; then
  echo "error: the stand-in service did not start" >&2
  cat "$WORKDIR/http.log" >&2 || true
  exit 1
fi

echo "==> writing the smoke profile"
mkdir -p "$CONFIG_DIR/scripts/default"
cat >"$CONFIG_DIR/config.toml" <<TOML
[settings]
locale = "en"
display_name = "127.0.0.1:$PORT"
default_profile = "default"

[settings.window]
width = 1100
height = 760

[profile.default]
id = "default"
name = "127.0.0.1:$PORT"
url = "http://127.0.0.1:$PORT"
prelaunch = "run"
TOML

cat >"$CONFIG_DIR/scripts/default/pre-launch.sh" <<'SH'
#!/usr/bin/env sh
# Smoke-test hook: proves the hook runner executes before navigation.
echo "smoke hook running for $TAURI_3080_PROFILE ($TAURI_3080_URL)"
SH
chmod +x "$CONFIG_DIR/scripts/default/pre-launch.sh"

echo "==> running the app"
#
# CI always has xvfb-run. Developers with a working display can run the script
# without Xvfb; a headless machine without it is told what to install.
if command -v xvfb-run >/dev/null 2>&1; then
  RUNNER=(xvfb-run -a --server-args='-screen 0 1280x800x24' dbus-run-session --)
elif [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  echo "note: xvfb-run not found, using the current display"
  RUNNER=(dbus-run-session --)
else
  echo "error: install xvfb-run (package 'xvfb') or run with a display" >&2
  exit 1
fi

# WebKitGTK, GTK and the tray all write below $HOME / $XDG_RUNTIME_DIR. Point
# them at the throw-away workspace so the smoke run never touches the real
# profile and works even when $HOME is read-only (containers, sandboxes).
mkdir -p "$WORKDIR/home" "$WORKDIR/runtime" "$WORKDIR/cache" "$WORKDIR/data"
chmod 700 "$WORKDIR/runtime"

HOME="$WORKDIR/home" \
XDG_RUNTIME_DIR="$WORKDIR/runtime" \
XDG_CACHE_HOME="$WORKDIR/cache" \
XDG_DATA_HOME="$WORKDIR/data" \
TAURI_3080_SMOKE="$REPORT" \
TAURI_3080_CONFIG_HOME="$CONFIG_DIR" \
WEBKIT_DISABLE_COMPOSITING_MODE=1 \
GDK_BACKEND=x11 \
  "${RUNNER[@]}" "$BINARY"

if [ ! -s "$REPORT" ]; then
  echo "error: no smoke report was written" >&2
  exit 1
fi

echo "==> smoke report"
cat "$REPORT"
echo

python3 - "$REPORT" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    report = json.load(handle)

required = ("window_created", "frontend_ready", "ipc_ok", "config_loaded", "navigated")
failed = [key for key in required if not report.get(key)]
if failed or not report.get("ok"):
    print("smoke check failed:", ", ".join(failed) or "ok=false", file=sys.stderr)
    sys.exit(1)

print(f"smoke OK in {report['durations_ms']['total']} ms on {report['platform']}")
PY
