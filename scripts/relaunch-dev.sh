#!/usr/bin/env bash
# Hard relaunch for Espada3.7 debug:
# - Debug binary uses cfg(dev) → loads UI from http://127.0.0.1:1420 (NOT embedded files)
# - So we MUST kill the old binary, kill the old server, clear WebKit cache,
#   start a no-cache server on the real ui/ folder, then start the binary.
set -euo pipefail

export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
export PATH="$DEVELOPER_DIR/usr/bin:/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/ui"
BIN="$ROOT/src-tauri/target/debug/espada3_7"
LOG="/tmp/espada-relaunch.log"
SRV_LOG="/tmp/espada-ui-server.log"
PORT=1420

echo "ROOT=$ROOT"
echo "UI=$UI"

kill_port() {
  local port="$1"
  local pids
  pids=$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo "==> Killing listeners on :$port → $pids"
    # shellcheck disable=SC2086
    kill $pids 2>/dev/null || true
    sleep 0.3
    # shellcheck disable=SC2086
    kill -9 $pids 2>/dev/null || true
  fi
}

echo "==> Killing Espada binaries (exact name)..."
pkill -x espada3_7 2>/dev/null || true
pkill -9 -x espada3_7 2>/dev/null || true
sleep 0.2

echo "==> Killing anything on port $PORT (dev UI server)..."
kill_port "$PORT"

# Also kill leftover python http.server whose cwd was Espada (best-effort)
while read -r pid cmd; do
  case "$cmd" in
    *http.server*"$PORT"*)
      echo "  kill leftover server $pid"
      kill "$pid" 2>/dev/null || true
      kill -9 "$pid" 2>/dev/null || true
      ;;
  esac
done < <(ps -axo pid=,command= 2>/dev/null || true)

echo "==> Clearing WebKit / app caches so WKWebView cannot serve old HTML..."
rm -rf \
  "$HOME/Library/Caches/com.asignaciondelcielo.espada37" \
  "$HOME/Library/Caches/espada3_7" \
  "$HOME/Library/WebKit/com.asignaciondelcielo.espada37/WebsiteData/Default"/*/CacheStorage \
  "$HOME/Library/WebKit/com.asignaciondelcielo.espada37/WebsiteData/Default"/*/NetworkCache \
  "$HOME/Library/WebKit/espada3_7/WebsiteData/Default"/*/CacheStorage \
  "$HOME/Library/WebKit/espada3_7/WebsiteData/Default"/*/NetworkCache \
  2>/dev/null || true

echo "==> Starting no-cache UI server from ABSOLUTE path: $UI"
# Custom SimpleHTTPRequestHandler with Cache-Control: no-store
: >"$SRV_LOG"
python3 - "$UI" "$PORT" >>"$SRV_LOG" 2>&1 <<'PY' &
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from functools import partial

ui_dir = sys.argv[1]
port = int(sys.argv[2])

class NoCacheHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()

    def log_message(self, fmt, *args):
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

handler = partial(NoCacheHandler, directory=ui_dir)
httpd = ThreadingHTTPServer(("127.0.0.1", port), handler)
print(f"Espada UI server → http://127.0.0.1:{port}/  dir={ui_dir}", flush=True)
httpd.serve_forever()
PY
SRV_PID=$!
echo "  server PID $SRV_PID"
sleep 0.5

# Prove server is correct
echo "==> Proving server content..."
if ! curl -fsS "http://127.0.0.1:$PORT/index.html" | grep -q "build-frost5"; then
  echo "ERROR: server is NOT serving the new index.html (missing build-frost5 stamp)" >&2
  echo "--- server log ---" >&2
  cat "$SRV_LOG" >&2 || true
  echo "--- curl head ---" >&2
  curl -sS "http://127.0.0.1:$PORT/index.html" | head -20 >&2 || true
  exit 1
fi
echo "  OK: index has build-frost5 stamp"
: > /tmp/espada-frost.log

echo "==> Building debug binary..."
cd "$ROOT/src-tauri"
cargo build 2>&1 | tee "$LOG" | tail -20

if [ ! -x "$BIN" ]; then
  echo "ERROR: missing $BIN" >&2
  exit 1
fi

echo "==> Launching binary (dev mode → http://127.0.0.1:$PORT)"
# Do not background with path patterns that trip tool guards in some shells
nohup "$BIN" >>"$LOG" 2>&1 &
APP_PID=$!
echo "  app PID $APP_PID"
sleep 1.0

if kill -0 "$APP_PID" 2>/dev/null; then
  echo "OK — Espada running"
  echo "Look at the titlebar: «build-frost5» — drag the titlebar to move the window"
  echo "Frosted Glass: more transparent; stamp → frost-on:hud"
  echo "Server log: $SRV_LOG"
  echo "App log:    $LOG"
  echo "Frost log:  /tmp/espada-frost.log"
else
  echo "FAILED to keep app alive" >&2
  tail -40 "$LOG" >&2 || true
  exit 1
fi
