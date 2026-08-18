#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_TOML="$ROOT_DIR/rust/Cargo.toml"

current="$(python3 -c 'import re; m=re.search(r"^version\s*=\s*\"([^\"]+)\"", open("rust/Cargo.toml").read(), re.MULTILINE); print(m.group(1) if m else "")' 2>/dev/null || true)"
if [[ -z "$current" ]]; then
  current="$(grep -m1 -E '^version = ' "$CARGO_TOML" | sed -E 's/^version = "(.*)"$/\1/')"
fi

echo "当前版本: $current"
read -r -p '请输入新的版本号 (如 0.1.2): ' new
if [[ ! "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "[error] 无效版本号: $new"
  exit 1
fi
if [[ "$new" == "$current" ]]; then
  echo "[skip] 新版本号与当前相同"
  exit 0
fi

echo "[bump] rust/Cargo.toml: $current -> $new"
sed -i '' -E "s/^version = \"$current\"/version = \"$new\"/" "$CARGO_TOML"
grep -q "^version = \"$new\"" "$CARGO_TOML" || { echo "[error] 版本号写入失败"; exit 1; }

echo "[sync] 同步 npm / pypi 版本号"
uv_bin="$(command -v uv || echo "$HOME/.local/bin/uv")"
"$uv_bin" run python "$ROOT_DIR/scripts/release/sync_version.py"

echo "[lock] 刷新 Cargo.lock"
( cd "$ROOT_DIR/rust" && cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null )

echo "[done] 版本已同步到 $new"
echo "  下一步: 提交变更并打 tag v$new 推送,触发 release-build"
