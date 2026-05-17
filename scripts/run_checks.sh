#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml --features python-module
python/.venv/bin/python -m unittest discover -s python/tests -v
python/.venv/bin/python python/examples/basic_usage.py
python/.venv/bin/python python/examples/webpage_modes.py
cargo run --manifest-path rust/Cargo.toml --example webpage_modes
