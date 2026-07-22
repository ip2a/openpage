#!/usr/bin/env bash
set -e

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

printf '%s\n' \
  'OpenPage Debug' \
  '1) 启动 App'
read -r -p '请选择: ' choice

case "$choice" in
  1)
    export OPENPAGE_BIN="$ROOT_DIR/rust/target/debug/openpage"
    export OPENPAGE_HOME="${OPENPAGE_DEBUG_HOME:-/tmp/openpage-desktop-debug-${USER:-user}}"

    cargo build --manifest-path "$ROOT_DIR/rust/apps/openpage/Cargo.toml" --bin openpage
    "$OPENPAGE_BIN" browser stop --session default >/dev/null 2>&1 || true

    cd "$ROOT_DIR/desktop/openpage"
    npm run tauri -- dev
    ;;
  *)
    echo '无效选项'
    exit 1
    ;;
esac
