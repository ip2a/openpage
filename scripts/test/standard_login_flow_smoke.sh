#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  printf '[error] Usage: %s <path-to-openpage-binary>\n' "$0" >&2
  exit 1
fi

BINARY="$1"
if [ ! -x "$BINARY" ]; then
  printf '[error] Binary is not executable: %s\n' "$BINARY" >&2
  exit 1
fi

FIXTURE_DIR="$(mktemp -d)"
SESSION="standard-login-smoke-$$"
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
      <h1>Sign in</h1>
      <form action="/products.html">
        <input name="username" placeholder="Username" required>
        <input name="password" type="password" placeholder="Password" required>
        <button type="submit">Login</button>
      </form>
    </main>
  </body>
</html>
HTML

cat >"$FIXTURE_DIR/products.html" <<'HTML'
<!doctype html>
<html lang="en">
  <body>
    <main>
      <h1>Products</h1>
      <div><div><div><div><div><div><div><div><div><div>
        <article class="product"><h2>Widget A</h2><p>$19.99</p></article>
        <article class="product"><h2>Widget B</h2><p>$29.99</p></article>
      </div></div></div></div></div></div></div></div></div></div>
    </main>
  </body>
</html>
HTML

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$FIXTURE_DIR" \
  >"$FIXTURE_DIR/server.log" 2>&1 &
SERVER_PID="$!"

"$BINARY" browser start --session "$SESSION" --headless "http://127.0.0.1:$PORT/" >/dev/null
"$BINARY" wait-for-ready --session "$SESSION" >/dev/null

snapshot="$($BINARY snapshot --session "$SESSION" --mode interactive --format json --compact)"
read -r username_ref password_ref login_ref < <(
  printf '%s' "$snapshot" | python3 -c '
import json, sys
rows = json.load(sys.stdin)["result"]["snapshot"]
refs = {row["name"]: row["ref"] for row in rows}
print(refs["Username"], refs["Password"], refs["Login"])
'
)

"$BINARY" fill "@$username_ref" standard_user --session "$SESSION" >/dev/null
printf '%s' 'secret-value' | "$BINARY" fill "@$password_ref" --stdin --session "$SESSION" >/dev/null
navigation="$($BINARY click "@$login_ref" --wait-navigation --session "$SESSION")"
printf '%s' "$navigation" | python3 -c '
import json, sys
result = json.load(sys.stdin)["result"]
assert result["clicked"] is True
assert result["navigation"]["ready"] is True
assert "/products.html" in result["navigation"]["url"]
'

products="$($BINARY snapshot --session "$SESSION" --mode semantic --format json --compact)"
printf '%s' "$products" | python3 -c '
import json, sys
rows = json.load(sys.stdin)["result"]["snapshot"]
names = {row["name"] for row in rows}
assert {"Products", "Widget A", "Widget B", "$19.99", "$29.99"} <= names
'

count="$($BINARY count '.product' --session "$SESSION")"
printf '%s' "$count" | python3 -c 'import json,sys; assert json.load(sys.stdin)["result"]["count"] == 2'
printf '[ok] Standard login flow passed\n'
