#!/usr/bin/env bash
set -e

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"

printf '%s\n' \
  'OpenPage Debug' \
  '1) 启动 App'
read -r -p '请选择: ' choice

case "$choice" in
  1)
    cd "$ROOT_DIR/desktop/openpage"
    npm run tauri -- dev
    ;;
  *)
    echo '无效选项'
    exit 1
    ;;
esac
