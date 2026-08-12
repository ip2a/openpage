#!/usr/bin/env bash
set -e

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
INTERNAL_DIR="$ROOT_DIR/npm/packages/internal"
MAIN_PKG="$ROOT_DIR/npm/packages/openpage"
PLATFORM_PKGS="openpage-bin-darwin-arm64 openpage-bin-darwin-x64 openpage-bin-linux-x64-gnu openpage-bin-linux-arm64-gnu openpage-bin-win32-x64-msvc"

# detect current platform -> internal npm package name (mirrors bin/openpage.js)
npm_platform_pkg() {
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64|Darwin-aarch64) echo "openpage-bin-darwin-arm64" ;;
    Darwin-x86_64|Darwin-amd64)  echo "openpage-bin-darwin-x64" ;;
    Linux-x86_64|Linux-amd64)    echo "openpage-bin-linux-x64-gnu" ;;
    Linux-aarch64|Linux-arm64)   echo "openpage-bin-linux-arm64-gnu" ;;
    *) return 1 ;;
  esac
}

# ── 构建 ────────────────────────────────────────────────────────────────────
# 仅编译 debug 二进制,不装到任何地方
build_debug() {
  echo "[build] debug binary -> rust/target/debug/openpage"
  cargo build --manifest-path "$ROOT_DIR/rust/apps/openpage/Cargo.toml" --bin openpage
  echo "[done] $ROOT_DIR/rust/target/debug/openpage"
}

# ── cargo 渠道(release → ~/.cargo/bin)──────────────────────────────────────
cargo_install() {
  echo "[cargo install] rust/apps/openpage (release)"
  cargo install --path "$ROOT_DIR/rust/apps/openpage" --bin openpage
  echo "[done] openpage -> $(command -v openpage)"
  echo "  安装记录为包名 openpage-app;卸载用对应菜单项"
}

cargo_uninstall() {
  echo "[cargo uninstall] openpage-app"
  cargo uninstall openpage-app 2>&1 || echo "[skip] openpage-app 未安装"
  if command -v openpage >/dev/null 2>&1; then
    echo "[warn] openpage 仍在 PATH: $(command -v openpage)(可能来自 npm link,见 npm 渠道)"
  else
    echo "[done] openpage 已从 PATH 移除"
  fi
}

# ── npm 渠道(本地模拟发布后:debug 二进制 → 内部包 → 全局 link)─────────────
npm_link() {
  local pkg; pkg="$(npm_platform_pkg)" || { echo "[error] unsupported platform: $(uname -s) $(uname -m)"; exit 1; }
  echo "[build] debug binary"
  cargo build --manifest-path "$ROOT_DIR/rust/apps/openpage/Cargo.toml" --bin openpage
  echo "[seed] -> $pkg/bin/openpage"
  mkdir -p "$INTERNAL_DIR/$pkg/bin"
  cp "$ROOT_DIR/rust/target/debug/openpage" "$INTERNAL_DIR/$pkg/bin/openpage"
  chmod +x "$INTERNAL_DIR/$pkg/bin/openpage"
  echo "[link] $pkg -> main"
  ( cd "$INTERNAL_DIR/$pkg" && npm link --silent )
  ( cd "$MAIN_PKG" && npm link "$pkg" --silent && npm link --silent )
  echo "[done] openpage -> $(command -v openpage)"
  echo "  测试: npx -y openpage mcp --session agent"
}

npm_unlink() {
  local pkg; pkg="$(npm_platform_pkg)" || pkg=""
  echo "[unlink] npm 全局 link"
  # shellcheck disable=SC2086
  npm uninstall -g openpage $PLATFORM_PKGS 2>/dev/null || true
  if [[ -n "$pkg" && -f "$INTERNAL_DIR/$pkg/bin/openpage" ]]; then
    rm -f "$INTERNAL_DIR/$pkg/bin/openpage"
    echo "[rm] $pkg/bin/openpage (seed 的二进制)"
  fi
  if command -v openpage >/dev/null 2>&1; then
    echo "[warn] openpage 仍在 PATH: $(command -v openpage)(可能来自 cargo install,见 cargo 渠道)"
  else
    echo "[done] openpage 已从 PATH 移除"
  fi
}

# ── pypi 渠道 ────────────────────────────────────────────────────────────────
uninstall_pypi() {
  local python_bin
  if [[ -x "$ROOT_DIR/python/.venv/bin/python" ]]; then
    python_bin="$ROOT_DIR/python/.venv/bin/python"
  elif [[ -x "$ROOT_DIR/.venv/bin/python" ]]; then
    python_bin="$ROOT_DIR/.venv/bin/python"
  else
    echo "[skip] 未找到项目虚拟环境: python/.venv 或 .venv"
    return
  fi
  echo "[unlink] $python_bin: openpage + openpage-rs"
  uv pip uninstall --python "$python_bin" openpage openpage-rs 2>&1 || true
  echo "[done] 已卸载 openpage + openpage-rs"
}

printf '%s\n' \
  'OpenPage Debug' \
  '' \
  '  1) 启动 App (desktop tauri dev)' \
  '' \
  '  [构建]' \
  '  2) 单独构建 debug 二进制 (rust/target/debug/openpage)' \
  '' \
  '  [cargo 渠道 · release 装到 ~/.cargo/bin]' \
  '  3) cargo 构建并安装' \
  '  4) cargo 卸载' \
  '' \
  '  [npm 渠道 · 本地模拟发布后]' \
  '  5) 构建并复制到 npm 并 link' \
  '  6) 取消 npm 的 cp 和 link' \
  '' \
  '  [pypi 渠道]' \
  '  7) 构建并安装最新本地 Python wheel' \
  '  8) 卸载 pypi 本地安装 (openpage + openpage-rs)'
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
  2) build_debug ;;
  3) cargo_install ;;
  4) cargo_uninstall ;;
  5) npm_link ;;
  6) npm_unlink ;;
  7)
    cd "$ROOT_DIR"
    uv build --wheel
    WHEEL_PATH="$(python3 -c 'from pathlib import Path; print(max(Path("dist").glob("openpage-*.whl"), key=lambda p: p.stat().st_mtime))')"
    if [[ -x "$ROOT_DIR/python/.venv/bin/python" ]]; then
      PYTHON_BIN="$ROOT_DIR/python/.venv/bin/python"
    elif [[ -x "$ROOT_DIR/.venv/bin/python" ]]; then
      PYTHON_BIN="$ROOT_DIR/.venv/bin/python"
    else
      echo "[error] 未找到项目虚拟环境: python/.venv 或 .venv"
      echo "请先创建: uv venv python/.venv"
      exit 1
    fi
    uv pip install --python "$PYTHON_BIN" --force-reinstall --no-deps "$WHEEL_PATH"
    echo "[ok] 已安装本地 wheel: $WHEEL_PATH"
    echo "[ok] Python 环境: $PYTHON_BIN"
    ;;
  8) uninstall_pypi ;;
  *)
    echo '无效选项'
    exit 1
    ;;
esac
