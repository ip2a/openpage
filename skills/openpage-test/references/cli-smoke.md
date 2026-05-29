# CLI Smoke

## Current Branch Gate

Before any runtime smoke, rerun:

```bash
cargo check --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick
```

If either of these fails, stop and report that result first.

Latest recheck on `2026-05-30`:

- `cargo check --manifest-path rust/Cargo.toml` passed
- `openpage browser list` currently returns 6 healthy sessions and no incomplete or cleaned sidecars
- `openpage doctor --quick --fix` was run locally, and `openpage doctor --quick` no longer warns about legacy session JSON residue under `/Users/yuuu/.openpage/sessions`
- `openpage doctor --quick` currently fails at the browser executable check:
  - `browser_path=chrome`
  - configured browser executable not found on PATH
- for machine-local override work, the active CLI and doctor now also honor:
  - `OPENPAGE_BROWSER_PATH=/absolute/path/to/browser`
- `openpage doctor --quick` machine-readable summary now also carries actionable ID lists:
  - `warn_ids`
  - `fail_ids`
  - `info_ids`
  - `fixable_ids`
- `openpage doctor` reports the same browser executable/config failure and skips live launch after that
- with `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"` on this machine:
  - `openpage doctor --quick` passes
  - full `openpage doctor` passes, including the live headless launch smoke
  - `browser start --headless https://example.com -> title -> browser stop` also passes without editing repo defaults

Interpretation:

- The current branch is compilable.
- The TCP daemon path is not currently the blocked part.
- Legacy session JSON residue is not part of the active TCP daemon execution path, and `doctor --quick --fix` is now the repo-local cleanup path for it.
- Daemon inventory is currently healthy on this machine; the remaining red item is browser executable resolution.
- Session count is runtime state, not a repository invariant. Treat the current `browser list` output as authoritative, not any older hard-coded count in notes.
- If `doctor --quick` or full `doctor` fails this way on your machine, treat it as a local browser/config problem first.
- On the current macOS machine used for the latest audit, Chrome exists as an app bundle under `/Applications/Google Chrome.app`, but `chrome` is not on PATH.
- On that machine, `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"` is now the process-local workaround that does not require editing repo defaults.
- The removed legacy CLI surfaces are intentionally rejected and now have parser tests:
  - `serve --stdio`
  - `page get`
  - `page url`
  - `page title`
  - `page screenshot`
- Runtime JSON failures now also expose stable `error.kind` values. For automation, prefer matching on:
  - `unsupported_operation`
  - `browser_operation`
  - `timeout`
  - `io`
  rather than scraping the human message text.

## Last Successful Runtime Observations

When the crate built successfully on `2026-05-29`, the following runtime behavior was confirmed:

- TCP daemon path: open page, read title, save screenshot, and the screenshot is visually correct
- named-session CLI: `browser start`, `goto`, `url`, `title`, and `screenshot` succeed through the same TCP daemon-backed control path
- AI-first snapshot ref flow: `snapshot` returns structured `eN` refs plus `text` / `refs` metadata, and `click @e1` works through the daemon path
- outer-shell borrowed features now available:
  - `batch`
  - `doctor`
  - output boundaries via `OPENPAGE_CONTENT_BOUNDARIES` / `OPENPAGE_MAX_OUTPUT_CHARS`

Interpretation:

- Rust CLI is usable without Python.
- the TCP daemon path is the higher-confidence agent-control path.
- named-session CLI commands now use that same TCP daemon-backed execution path.

## Preferred Smoke Test: TCP daemon

Use the helper script:

```bash
bash skills/openpage-test/scripts/serve_baidu_smoke.sh
```

Manual form:

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --session smoke
```

Then connect over TCP and send NDJSON:

```json
{"id":"1","op":"webpage.create","target":"smoke","params":{"headless":true}}
{"id":"2","op":"webpage.get","target":"smoke","params":{"url":"https://www.baidu.com"}}
{"id":"3","op":"webpage.title","target":"smoke"}
{"id":"4","op":"page.screenshot","target":"smoke","params":{"path":"/tmp/openpage-cli-artifacts/serve-baidu.png"}}
{"id":"5","op":"daemon.shutdown"}
```

Expected result:

- title includes `百度一下，你就知道`
- screenshot exists at `/tmp/openpage-cli-artifacts/serve-baidu.png`
- the screenshot visibly shows the Baidu homepage

Before calling the daemon path broken, run:

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor
```

If `doctor` says the configured browser executable cannot be found, fix that first.

## Secondary Smoke Test: Named-Session CLI

Use the helper script:

```bash
bash skills/openpage-test/scripts/named_session_baidu_smoke.sh
```

Manual form:

```bash
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session review --replace --headless
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- goto https://www.baidu.com --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- url --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- title --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- screenshot /tmp/openpage-cli-artifacts/review-baidu.png --session review
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --session review
```

Current behavior:

- named-session CLI commands now route through the same TCP daemon-backed execution path
- if the screenshot is blank or white, count the run as failed even if `saved: true` is returned
- `batch` can be used to collapse the same smoke into one invocation if needed

Example:

```bash
OPENPAGE_HOME=/tmp/openpage-cli-test cargo run --manifest-path rust/Cargo.toml --bin openpage -- batch \
  "browser start https://www.baidu.com --headless --session review" \
  "title --session review" \
  "screenshot /tmp/openpage-cli-artifacts/review-baidu.png --session review" \
  "browser stop --session review"
```

## Screenshot Verification

Do all three checks:

```bash
ls -lh /tmp/openpage-cli-artifacts/serve-baidu.png
file /tmp/openpage-cli-artifacts/serve-baidu.png
ls -lh /tmp/openpage-cli-artifacts/review-baidu.png
file /tmp/openpage-cli-artifacts/review-baidu.png
```

Then visually inspect the images.

Rules:

- file exists only: not enough
- PNG metadata looks correct: still not enough
- visible page content matches the target site: required

## Common Failure Meanings

- `could not find a Chrome/Chromium executable`
  - pass `--browser-path` to `browser start` or `webpage.create`
- `doctor` says `Configured browser executable "chrome" was not found`
  - edit `rust/configs.ini` `browser_path`
  - or install the browser so that `chrome` resolves on PATH
  - or pass `--browser-path` explicitly in smoke commands
- named-session CLI failure but raw TCP daemon smoke passes
  - treat that as a CLI-wrapper regression first, not a transport-path failure
- screenshot saved but blank/white
  - rendering or attach context is not healthy enough for reliable automation
- Python-only failure after Rust passes
  - wrapper/install path needs work; do not call it a Rust-core failure by default
