#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "[error] Usage: $0 <path-to-binary>"
  exit 1
fi

BINARY="$1"

if [ ! -f "${BINARY}" ]; then
  echo "[error] Binary not found: ${BINARY}"
  exit 1
fi

echo "[run] Smoke test: ${BINARY} --help"
"${BINARY}" --help >/dev/null

echo "[run] Smoke test: ${BINARY} --version"
"${BINARY}" --version

echo "[run] Smoke test: ${BINARY} mcp"
payload=$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | "${BINARY}" mcp --session smoke)
printf '%s\n' "$payload" | python3 -c 'import json,sys; row=json.loads(sys.stdin.read()); assert row["result"]["serverInfo"]["name"] == "openpage"'

echo "[ok] Smoke test passed"
