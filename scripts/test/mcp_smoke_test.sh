#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

cargo build --quiet --manifest-path rust/Cargo.toml --bin openpage
BINARY="$ROOT_DIR/rust/target/debug/openpage"
FIXTURE_DIR="$(mktemp -d)"
SESSION="mcp-smoke-$$"
PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
CDP_PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
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
      <input id="continue" type="button" value="Continue" style="width:0;height:0;border:0;padding:0" onclick="document.title='fallback-clicked'">
      <select name="country"><option>China</option><option>United States</option></select>
      <a id="blank" href="./opened.html" target="_blank">Open new tab</a>
  <iframe src="./frame.html" title="nested"></iframe>
    </main>
  </body>
</html>
HTML

cat >"$FIXTURE_DIR/opened.html" <<'HTML'
<!doctype html>
<html><body><h1>Opened</h1></body></html>
HTML

cat >"$FIXTURE_DIR/frame.html" <<'HTML'
<!doctype html>
<html><body><h1>Frame</h1></body></html>
HTML

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$FIXTURE_DIR" \
  >"$FIXTURE_DIR/server.log" 2>&1 &
SERVER_PID="$!"

"$BINARY" browser start --session "$SESSION" --headless --port "$CDP_PORT" --user-data-dir "$FIXTURE_DIR/profile" >/dev/null
"$BINARY" goto --session "$SESSION" --wait "http://127.0.0.1:$PORT/" >/dev/null
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
  '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"openpage","arguments":{"command":"help browser"}}}' \
  '{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"openpage","arguments":{"command":"browser list"}}}' \
  '{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"fill","arguments":{"locator":"@e5","text":"smoke"}}}' \
  '{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"snapshot","arguments":{"mode":"interactive","exclude_roles":["option"]}}}' \
  '{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"click","arguments":{"locator":"#continue"}}}' \
  '{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"openpage","arguments":{"command":"title"}}}' \
  '{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"click","arguments":{"locator":"@e5","wait_until":"domcontentloaded"}}}' \
  '{"jsonrpc":"2.0","id":17,"method":"tools/call","params":{"name":"openpage","arguments":{"command":"frame switch 1"}}}' \
  '{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"openpage","arguments":{"command":"frame switch main"}}}' \
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
assert len(rows) == 18, len(rows)
by_id = {row["id"]: row for row in rows}

initialize = by_id[1]["result"]
assert initialize["protocolVersion"] == "2025-06-18", initialize
assert initialize["serverInfo"]["name"] == "openpage", initialize
capabilities = initialize["capabilities"]
resources_caps = capabilities["resources"]
prompts_caps = capabilities["prompts"]
if resources_caps.get("listChanged") is not False or resources_caps.get("subscribe") is not False or prompts_caps.get("listChanged") is not False:
    raise RuntimeError(capabilities)

tools = by_id[2]["result"]["tools"]
expected_tools = {"help", "openpage", "snapshot", "screenshot", "navigate", "click", "fill"}
assert len(tools) == 7, tools
assert {tool["name"] for tool in tools} == expected_tools
assert all(tool.get("outputSchema") for tool in tools)
for name in ("snapshot", "screenshot", "navigate", "click", "fill"):
    properties = next(tool for tool in tools if tool["name"] == name)["inputSchema"].get("properties", {})
    assert "session" not in properties, name
navigate = next(tool for tool in tools if tool["name"] == "navigate")["inputSchema"]["properties"]
assert "wait_until" in navigate, navigate
assert "wait" not in navigate, navigate

snapshot = by_id[3]["result"]
assert snapshot["isError"] is False
assert snapshot["structuredContent"]["ok"] is True
snapshot_result = snapshot["structuredContent"]["result"]
assert snapshot_result["revision"].startswith("r_")
assert isinstance(snapshot_result["refs"], dict)
assert any(item["type"] == "resource_link" for item in snapshot["content"])
snapshot_text = json.loads(snapshot["content"][0]["text"])
assert snapshot_text == snapshot_result
assert snapshot_text["refs"]

stale = by_id[4]["result"]
assert stale["isError"] is True
# @e999 with r_stale: revision mismatch fires first (page state changed).
assert stale["structuredContent"]["error"]["kind"] == "element_not_found"
assert stale["structuredContent"]["error"]["failure_reason"] == "revision_mismatch"
assert stale["structuredContent"]["error"]["suggested_action"] == "retry_without_revision"

batch = by_id[5]["result"]["structuredContent"]
assert batch["ok"] is True
commands = batch["result"]["commands"]
assert len(commands) == 2
assert all({"index", "command", "ok"} <= command.keys() for command in commands)
assert all(command["ok"] is True for command in commands)
assert json.loads(by_id[5]["result"]["content"][0]["text"])["commands"] == commands

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

help_result = by_id[10]["result"]
assert help_result["isError"] is False
assert help_result["structuredContent"]["ok"] is True
assert "Manage the browser session" in help_result["content"][0]["text"]

browser_list = by_id[11]["result"]
assert browser_list["isError"] is False
assert browser_list["structuredContent"]["ok"] is True
assert json.loads(browser_list["content"][0]["text"]) == browser_list["structuredContent"]["result"]

fill = by_id[12]["result"]
assert fill["isError"] is False, fill

filtered = by_id[13]["result"]["structuredContent"]["result"]
assert filtered["exclude_roles"] == ["option"]
assert filtered["excluded_count"] == 2
assert all(entry["role"].lower() != "option" for entry in filtered["snapshot"])
# Refs survive separate MCP tool invocations/CLI processes.
surviving_keys = set(snapshot_result["refs"]) - {ref for ref, value in snapshot_result["refs"].items() if value["role"] == "option"}
assert set(filtered["refs"]) == surviving_keys, (filtered["refs"], surviving_keys)

no_rect = by_id[14]["result"]
assert no_rect["isError"] is False, no_rect["structuredContent"]

title = by_id[15]["result"]
assert title["isError"] is False, title["structuredContent"]
assert title["structuredContent"]["result"]["title"] == "fallback-clicked", title["structuredContent"]

opened_tab = by_id[16]["result"]
assert opened_tab["isError"] is False, opened_tab["structuredContent"]
click_result = opened_tab["structuredContent"]["result"]
assert click_result["navigation"]["opened_tab"] is True, click_result
assert click_result["navigation"]["target_id"], click_result

frame_switch = by_id[17]["result"]
assert frame_switch["isError"] is False, frame_switch["structuredContent"]
assert frame_switch["structuredContent"]["result"]["frame_id"], frame_switch["structuredContent"]
main_switch = by_id[18]["result"]
assert main_switch["isError"] is False, main_switch["structuredContent"]
assert main_switch["structuredContent"]["result"]["frame"] == "main", main_switch["structuredContent"]
PY

echo "[ok] MCP stdio smoke test passed"
