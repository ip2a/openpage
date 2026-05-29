#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT_DIR"

ARTIFACT_DIR="${OPENPAGE_ARTIFACT_DIR:-/tmp/openpage-cli-artifacts}"
SCREENSHOT_PATH="${1:-$ARTIFACT_DIR/serve-baidu.png}"
mkdir -p "$ARTIFACT_DIR"

OPENPAGE_HOME_DIR="${OPENPAGE_HOME:-/tmp/openpage-cli-test-daemon}"
export OPENPAGE_HOME="$OPENPAGE_HOME_DIR"
SESSION="smoke"
LOG_PATH="$(mktemp)"

resolve_browser_path() {
  if [[ -n "${OPENPAGE_BROWSER_PATH:-}" ]]; then
    printf '%s\n' "$OPENPAGE_BROWSER_PATH"
    return 0
  fi
  for candidate in chrome google-chrome chromium chromium-browser; do
    if command -v "$candidate" >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

BROWSER_PATH="$(resolve_browser_path || true)"

cargo build --quiet --manifest-path rust/Cargo.toml --bin openpage
rust/target/debug/openpage serve --session "$SESSION" >"$LOG_PATH" 2>&1 &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

PORT=""
for _ in $(seq 1 50); do
  if [[ -s "$LOG_PATH" ]]; then
    PORT="$(python3 - <<'PY' "$LOG_PATH"
import json, sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.strip()
        if not line or not line.startswith("{"):
            continue
        data = json.loads(line)
        listening = data.get("listening", "")
        print(listening.rsplit(":", 1)[-1] if ":" in listening else "")
        raise SystemExit(0)
print("")
PY
)"
    if [[ -n "$PORT" ]]; then
      break
    fi
  fi
  sleep 0.2
done

if [[ -z "$PORT" ]]; then
  echo "[error] failed to discover daemon port" >&2
  cat "$LOG_PATH" >&2 || true
  exit 1
fi

python3 - <<'PY' "$PORT" "$SCREENSHOT_PATH" "$SESSION" "$BROWSER_PATH"
import json, socket, sys
port = int(sys.argv[1])
path = sys.argv[2]
session = sys.argv[3]
browser_path = sys.argv[4]
params = {"headless": True}
if browser_path:
    params["browser_path"] = browser_path
commands = [
    {"id":"1","op":"webpage.create","target":session,"params":params},
    {"id":"2","op":"webpage.get","target":session,"params":{"url":"https://www.baidu.com"}},
    {"id":"3","op":"webpage.title","target":session,"params":None},
    {"id":"4","op":"page.screenshot","target":session,"params":{"path":path}},
    {"id":"5","op":"daemon.shutdown","params":None},
]
with socket.create_connection(("127.0.0.1", port), timeout=10) as sock:
    fh = sock.makefile("rwb")
    for cmd in commands:
        fh.write((json.dumps(cmd) + "\n").encode("utf-8"))
        fh.flush()
        line = fh.readline().decode("utf-8").strip()
        if not line:
            raise SystemExit("empty response from daemon")
        print(line)
PY

echo "[ok] screenshot: $SCREENSHOT_PATH"
