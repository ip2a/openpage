#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT_DIR"

export OPENPAGE_HOME="${OPENPAGE_HOME:-/tmp/openpage-cli-test}"
ARTIFACT_DIR="${OPENPAGE_ARTIFACT_DIR:-/tmp/openpage-cli-artifacts}"
SESSION_NAME="${OPENPAGE_SESSION:-review}"
SCREENSHOT_PATH="${1:-$ARTIFACT_DIR/review-baidu.png}"
mkdir -p "$ARTIFACT_DIR"

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

run_openpage() {
  cargo run --manifest-path rust/Cargo.toml --bin openpage -- "$@"
}

cleanup() {
  run_openpage browser stop --session "$SESSION_NAME" >/dev/null 2>&1 || true
}

trap cleanup EXIT

START_ARGS=(browser start --session "$SESSION_NAME" --replace --headless)
if [[ -n "$BROWSER_PATH" ]]; then
  START_ARGS+=(--browser-path "$BROWSER_PATH")
fi
run_openpage "${START_ARGS[@]}"

run_openpage goto https://www.baidu.com --session "$SESSION_NAME"
run_openpage url --session "$SESSION_NAME"
run_openpage title --session "$SESSION_NAME"
run_openpage screenshot "$SCREENSHOT_PATH" --session "$SESSION_NAME"

echo "[ok] screenshot: $SCREENSHOT_PATH"
