#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT_DIR"

export OPENPAGE_HOME="${OPENPAGE_HOME:-/tmp/openpage-cli-test}"
ARTIFACT_DIR="${OPENPAGE_ARTIFACT_DIR:-/tmp/openpage-cli-artifacts}"
SESSION_NAME="${OPENPAGE_SESSION:-review}"
SCREENSHOT_PATH="${1:-$ARTIFACT_DIR/review-baidu.png}"
mkdir -p "$ARTIFACT_DIR"

run_openpage() {
  cargo run --manifest-path rust/Cargo.toml --bin openpage -- "$@"
}

cleanup() {
  run_openpage browser stop --session "$SESSION_NAME" >/dev/null 2>&1 || true
}

trap cleanup EXIT

run_openpage browser start --session "$SESSION_NAME" --replace --headless

set +e
GET_OUTPUT="$(run_openpage page get https://www.baidu.com --session "$SESSION_NAME" 2>&1)"
GET_STATUS=$?
set -e
printf '%s\n' "$GET_OUTPUT"
if [ "$GET_STATUS" -ne 0 ]; then
  echo "[warn] page get returned non-zero; continue with url/title/screenshot checks"
fi

run_openpage page url --session "$SESSION_NAME"
run_openpage page title --session "$SESSION_NAME"
run_openpage page screenshot "$SCREENSHOT_PATH" --session "$SESSION_NAME"

echo "[ok] screenshot: $SCREENSHOT_PATH"
