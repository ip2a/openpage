#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

payload=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  | cargo run --quiet --manifest-path rust/Cargo.toml --bin openpage -- mcp --session smoke)

printf '%s\n' "$payload" | python3 -c '
import json, sys
rows = [json.loads(line) for line in sys.stdin if line.strip()]
assert len(rows) == 2, rows
assert rows[0]["result"]["serverInfo"]["name"] == "openpage"
assert rows[1]["result"]["tools"][0]["name"] == "openpage"
'

echo "[ok] MCP stdio smoke test passed"
