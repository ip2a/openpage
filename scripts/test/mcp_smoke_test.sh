#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

cargo build --quiet --manifest-path rust/Cargo.toml --bin openpage
BINARY="$ROOT_DIR/rust/target/debug/openpage"
FIXTURE_DIR="$(mktemp -d)"
SESSION="mcp-smoke-$$"
PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
SERVER_PID=""

cleanup() {
  "$BINARY" browser stop --session "$SESSION" >/dev/null 2>&1 || true
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$FIXTURE_DIR"
}
trap cleanup EXIT

cat >"$FIXTURE_DIR/index.html" <<'HTML'
<!doctype html>
<html lang="en">
  <body>
    <main>
      <h1>MCP smoke</h1>
      <label>Name <input name="name"></label>
      <button type="button">Continue</button>
    </main>
  </body>
</html>
HTML

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$FIXTURE_DIR" \
  >"$FIXTURE_DIR/server.log" 2>&1 &
SERVER_PID="$!"

"$BINARY" browser start --session "$SESSION" --headless "http://127.0.0.1:$PORT/" >/dev/null
"$BINARY" wait-for-ready --session "$SESSION" >/dev/null

payload=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"snapshot","arguments":{"mode":"interactive"}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"click","arguments":{"locator":"@e999","expected_revision":"r_stale"}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"openpage","arguments":{"commands":["title","url"],"bail":true}}}' \
  '{"jsonrpc":"2.0","id":6,"method":"resources/list"}' \
  "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"resources/read\",\"params\":{\"uri\":\"openpage://sessions/$SESSION/captures/1\"}}" \
  '{"jsonrpc":"2.0","id":8,"method":"prompts/list"}' \
  '{"jsonrpc":"2.0","id":9,"method":"prompts/get","params":{"name":"review_page","arguments":{"focus":"all"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  | "$BINARY" mcp --session "$SESSION")

PAYLOAD_FILE="$FIXTURE_DIR/mcp.jsonl"
printf '%s\n' "$payload" >"$PAYLOAD_FILE"
python3 - "$SESSION" "$PAYLOAD_FILE" <<'PY'
import json
import sys

session = sys.argv[1]
with open(sys.argv[2], encoding="utf-8") as handle:
    rows = [json.loads(line) for line in handle if line.strip()]
assert len(rows) == 9, rows
by_id = {row["id"]: row for row in rows}

initialize = by_id[1]["result"]
assert initialize["protocolVersion"] == "2025-06-18"
assert initialize["serverInfo"]["name"] == "openpage"
assert initialize["capabilities"]["resources"] == {
    "listChanged": False,
    "subscribe": False,
}
assert initialize["capabilities"]["prompts"] == {"listChanged": False}

tools = by_id[2]["result"]["tools"]
expected_tools = {"help", "openpage", "snapshot", "screenshot", "navigate", "click", "fill"}
assert len(tools) == 7, tools
assert {tool["name"] for tool in tools} == expected_tools
assert all(tool.get("outputSchema") for tool in tools)
for name in ("snapshot", "screenshot", "navigate", "click", "fill"):
    properties = next(tool for tool in tools if tool["name"] == name)["inputSchema"].get("properties", {})
    assert "session" not in properties, name
navigate = next(tool for tool in tools if tool["name"] == "navigate")["inputSchema"]["properties"]
assert "wait_until" in navigate and "wait" not in navigate

snapshot = by_id[3]["result"]
assert snapshot["isError"] is False
assert snapshot["structuredContent"]["ok"] is True
snapshot_result = snapshot["structuredContent"]["result"]
assert snapshot_result["revision"].startswith("r_")
assert isinstance(snapshot_result["refs"], dict)
assert any(item["type"] == "resource_link" for item in snapshot["content"])

stale = by_id[4]["result"]
assert stale["isError"] is True
assert stale["structuredContent"]["error"]["kind"] == "stale_ref"
assert stale["structuredContent"]["error"]["suggested_action"] == "re-snapshot"

batch = by_id[5]["result"]["structuredContent"]
assert batch["ok"] is True
commands = batch["result"]["commands"]
assert len(commands) == 2
assert all({"index", "command", "ok"} <= command.keys() for command in commands)
assert all(command["ok"] is True for command in commands)

resources = by_id[6]["result"]["resources"]
assert len(resources) >= 1
uri = f"openpage://sessions/{session}/captures/1"
assert any(resource["uri"] == uri for resource in resources)
resource = by_id[7]["result"]["contents"][0]
assert resource["uri"] == uri
assert resource["mimeType"] == "application/json"
captured = json.loads(resource["text"])
assert captured["revision"] == snapshot_result["revision"]

prompts = by_id[8]["result"]["prompts"]
assert {prompt["name"] for prompt in prompts} == {
    "review_page",
    "collect_paginated_data",
    "guided_login",
}
messages = by_id[9]["result"]["messages"]
assert messages and messages[0]["content"]["text"]
PY

echo "[ok] MCP stdio smoke test passed"
