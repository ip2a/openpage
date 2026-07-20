#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml -- --test-threads=1

bash scripts/dev/dev_install.sh
python/.venv/bin/python tests/python/test_compat_download_wait.py
python/.venv/bin/python tests/python/test_openpage.py
