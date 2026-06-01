# CLI Smoke

## Current Branch Gate

Before any runtime smoke, rerun:

```bash
cargo check --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick
```

If either of these fails, stop and report that result first.

Latest recheck on `2026-05-31`:

- `cargo check --manifest-path rust/Cargo.toml` passed
- during the latest local recheck, `openpage browser list` returned only healthy sessions and no incomplete or cleaned sidecars
- at that exact recheck moment, the healthy sessions were:
  - `cli-more-states-2`
  - `cli-state-queries`
  - `clipboard-probe-20260531`
  - `definitely-missing`
  - `hist-smoke`
  - `human-clipboard-audit-20260531`
  - `human-flow`
  - `human-gap-check`
  - `human-window-audit-20260531`
  - `human-window-audit-20260531-b`
  - `human-window-audit-20260531-c`
  - `human-window-smoke-20260531`
  - `human-window-switch-20260531`
  - `local-audit-20260531-seq`
  - `local-audit-20260531-tcp`
  - `smoke-history2`
  - `smoke-shot`
  - `smoke_eval_5554`
- the current healthy session count at that recheck was 18
- this latest local recheck first needed a minimal compile repair in `rust/src/browser.rs` around tab/new-tab helper code before `cargo check` and runtime auditing could continue
- the exact healthy session count is runtime-local and drifts as named-session smoke daemons are created or left running
- do not infer failure from a session name alone: `definitely-missing` currently looks suspicious by name, but runtime evidence says it is alive and ready
- `openpage doctor --quick --fix` was run locally, and `openpage doctor --quick` no longer warns about legacy session JSON residue under `/Users/yuuu/.openpage/sessions`
- a synthetic `OPENPAGE_HOME=/tmp/openpage-doctor-fix-*` audit also verified that `doctor --quick --fix` now:
  - removes legacy session JSON residue
  - stops an incompatible version-mismatched daemon session
  - reports stale dead daemon sidecar cleanup
  - stops an incomplete unready daemon session
  - leaves no remaining sidecar files in that synthetic audit directory
- a separate synthetic `OPENPAGE_HOME=/tmp/openpage-stop-all-*` smoke also verified that:
  - two raw `openpage serve --session ... --port 0` daemons show up in `browser list`
  - `openpage browser stop --all` returns `stopped=2` and the expected session names
  - a follow-up `browser list` becomes empty
  - both daemon pids are no longer alive after the stop-all call
- `browser list` now also returns a machine-friendly `summary` object:
  - on the current machine recheck it reported `healthy=18`, `incompatible=0`, `incomplete=0`, `cleaned=0`, `total=18`
  - in a synthetic `OPENPAGE_HOME=/tmp/openpage-list-summary-*` smoke it reported `healthy=1`, `incompatible=0`, `incomplete=0`, `cleaned=0`, `total=1`
- current-machine `browser list` healthy entries now also each carry `state="healthy"`
- a version-mismatch smoke now also verifies:
  - after overwriting a live session's `.version` sidecar with `0.0.1`, `browser status --session ...` returns `state="incompatible"`
  - the same session then shows up in `browser list` with `summary.incompatible=1`
  - `browser status`, `browser logs`, and `browser list` now also surface the same stop/restart `fix`
  - a follow-up command such as `title --session ...` now fails fast with restart guidance instead of talking to the stale daemon
- when `browser list` reports incomplete sessions, each `incomplete[]` entry now carries:
  - `state="incomplete"`
  - stable `reasons[]`
- when `browser list` reports cleaned residue, each `cleaned[]` entry now carries `state="cleaned"`
- a synthetic `OPENPAGE_HOME=/tmp/openpage-status-shapes-*` smoke also verified that:
  - `browser status --session healthy` returns `state="healthy"`
  - `browser status --session mismatch` returns `state="incompatible"` plus `reasons=["version_mismatch"]`
  - `browser status --session incomplete` returns `state="incomplete"` plus `reasons=["missing_version","daemon_not_ready"]`
  - `browser status --session missing` returns `state="inactive"`
- `openpage doctor --quick` currently fails at the browser executable check:
  - in the current dirty worktree on this machine, `rust/configs.ini` currently resolves to `browser_path=/tmp/dp-browser`
  - that configured browser executable is not present on this machine
- the latest local quick-doctor summary is now:
  - `pass=22`
  - `warn=0`
  - `fail=1`
  - `info=1`
  - `total=24`
  - `fail_ids=["browser.executable"]`
  - `fixable_ids=["browser.executable","browser.launch"]`
- for machine-local override work, the active CLI and doctor now also honor:
  - `OPENPAGE_BROWSER_PATH=/absolute/path/to/browser`
- `openpage doctor --quick` machine-readable summary now also carries actionable ID lists:
  - `warn_ids`
  - `fail_ids`
  - `info_ids`
  - `fixable_ids`
- browser-related `doctor --quick` checks now also carry machine-readable browser-path fields:
  - `browser.config` and `browser.executable` carry `browser_path`
  - when the configured executable resolves, `browser.executable` also carries `resolved_path`
  - when doctor finds a usable local browser candidate for a missing alias such as `chrome`,
    `browser.executable` and `browser.executable.hint` carry `suggested_path`
- `openpage doctor --quick` now also carries a machine-readable `inventory` block:
  - when no daemon directory exists yet, it now stays an empty object instead of `null`
  - `summary.healthy=18`
  - `summary.incomplete=0`
  - `summary.cleaned=0`
  - `summary.total=18`
  - daemon-related `checks[]` entries now also carry `state/reasons` when the check is about a concrete session
  - those same daemon-related `checks[]` entries now also carry `session` directly
  - healthy `sessions[]` entries currently also carry `state="healthy"`
  - any future `incomplete[]` entries now use the same stable `reasons[]` taxonomy as `browser status` / `browser logs`
- `openpage doctor` reports the same browser executable/config failure and skips live launch after that
- with `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"` on this machine:
  - `openpage doctor --quick` passes
  - full `openpage doctor` passes, including the live headless launch smoke
  - `browser start --headless https://example.com -> title -> browser stop` also passes without editing repo defaults
- with a temporary project-local `dp_configs.ini` that sets `browser_path=chrome` on this machine:
  - `openpage doctor --quick` now returns
    `browser.executable.suggested_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
  - `browser.executable.hint` also carries the same `suggested_path`
- runtime browser creation now uses the same launch-config chain as `doctor`, so browser-start behavior and doctor browser-path checks should agree unless an explicit CLI/request override is passed
- on this machine, without `OPENPAGE_BROWSER_PATH`, `browser start --headless https://example.com` now fails with the same browser-path problem that `doctor` reports, which is the intended alignment

Interpretation:

- The current branch is compilable.
- The TCP daemon path is not currently the blocked part.
- Legacy session JSON residue is not part of the active TCP daemon execution path, and `doctor --quick --fix` is now the repo-local cleanup path for it, incompatible daemon sessions, and incomplete unready daemon sessions.
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
- Those removed surfaces and other top-level bad CLI inputs now return
  `error.kind="invalid_input"` in the same JSON shell as the rest of the active CLI.
- This was re-verified at runtime on `2026-05-31` with:
  - `openpage page url`
  - `openpage serve --stdio`
- Help and version output intentionally remain plain clap text rather than JSON.
- The compiled help text itself now carries the protocol truth:
  - `openpage --help` says the active CLI protocol is TCP-backed daemon only
  - `openpage serve --help` says the removed `serve --stdio` surface stays rejected
- `openpage browser logs --session ... --tail N` is now the active shell-level way to inspect a
  session's daemon log path and tailed stderr when a persisted log file exists.
  - it now also preserves the same shell-level `state` as `browser status`
  - if a session is incomplete, `browser logs` also preserves stable `reasons[]`
  - with `OPENPAGE_CONTENT_BOUNDARIES=1` / `OPENPAGE_MAX_OUTPUT_CHARS`, the log `content` field
    now also gets the same boundary / truncate treatment as page text output
- `openpage browser stop --all` is now the active shell-level way to close every daemon-backed
  session discovered in the current `OPENPAGE_HOME` without touching browser/CDP internals.
- The repo-local `dp` binary is only a DrissionPage compatibility helper for config and launch
  tasks. It is not a second daemon protocol or an alternate active CLI execution path.
- That compat-only constraint is now also guarded by the unit test
  `dp_compat_help_marks_surface_as_compat_only`, not just by README wording.
- `openpage --set-browser-path ...` and the other compat root flags are now rejected on the
  active `openpage` binary; use the `dp` helper binary if you intentionally need that compat surface.
- Runtime JSON failures and top-level input rejections now also expose stable `error.kind` values. For automation, prefer matching on:
  - `invalid_input`
  - `invalid_json`
  - `tcp_error`
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
- `browser start` and `goto` are the only commands that may bootstrap a missing named session
- follow-up session commands such as `title`, `snapshot`, `click`, `js`, and `screenshot` now fail fast if the named session is inactive
- if the screenshot is blank or white, count the run as failed even if `saved: true` is returned
- `batch` can be used to collapse the same smoke into one invocation if needed

Guardrail smoke:

```bash
OPENPAGE_HOME=/tmp/openpage-cli-missing cargo run --manifest-path rust/Cargo.toml --bin openpage -- title --session missing
```

Expected result:

- JSON failure with `error.kind="browser_operation"`
- the same top-level JSON error also carries `error.session="missing"` for known session-control failures
- the same top-level JSON error also carries `error.fix` when the CLI knows the next session action
- for known session-control failures, the same top-level JSON error also carries `error.state`
  and stable `error.reasons` when applicable
- when there is no known recovery step, `error.fix` is omitted instead of emitted as `null`
- message includes `browser start --session missing`
- no `missing.port` / `missing.pid` / `missing.version` files are created under `$OPENPAGE_HOME/daemon`

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
  - set `OPENPAGE_BROWSER_PATH=/absolute/path/to/browser`
  - or pass `--browser-path` to `browser start` or `webpage.create`
- `doctor` says `Configured browser executable "..." was not found`
- on the latest local recheck on `2026-05-30`, that exact configured path was `/tmp/dp-browser`
- `doctor` says the configured browser executable from `rust/configs.ini` was not found
  - set `OPENPAGE_BROWSER_PATH=/absolute/path/to/browser`
  - or edit `rust/configs.ini` `browser_path`
  - or install whatever executable name/path `doctor` is currently reporting
  - or pass `--browser-path` explicitly in smoke commands
- raw TCP daemon returns `error.kind="invalid_json"`
  - the client sent malformed NDJSON or a non-JSON line
- raw TCP daemon returns `error.kind="tcp_error"`
  - the daemon hit a transport-level failure while serving the TCP client
- named-session CLI failure but raw TCP daemon smoke passes
  - treat that as a CLI-wrapper regression first, not a transport-path failure
- a session looks alive but is still behaving strangely
  - run `openpage browser status --session <name>`
  - then run `openpage browser logs --session <name> --tail 20`
  - `exists=false` means there is no persisted stderr log for that session yet
- screenshot saved but blank/white
  - rendering or attach context is not healthy enough for reliable automation
- Python-only failure after Rust passes
  - wrapper/install path needs work; do not call it a Rust-core failure by default
