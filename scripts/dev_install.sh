#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

uv venv python/.venv

export VIRTUAL_ENV="$ROOT_DIR/python/.venv"
export PATH="$ROOT_DIR/python/.venv/bin:/Users/yuuu/.local/bin:/Users/yuuu/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"

uv tool run maturin develop --manifest-path rust/Cargo.toml --features python-module --uv
uv pip install --python python/.venv/bin/python -e python
