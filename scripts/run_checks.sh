#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

cargo check --manifest-path rust/Cargo.toml
python/.venv/bin/python -m unittest discover -s python/tests -v
