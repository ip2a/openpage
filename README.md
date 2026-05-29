# openpage

`openpage` is a Rust-first browser automation project with thin Python wrappers.

## Layout

- `rust/`: Rust core, direct examples, and optional PyO3 extension
- `python/`: Python wrappers, examples, and tests
- `参考项目/`: reference code used for API and architecture study

## Current architecture

- Rust owns:
  - browser launch/connect lifecycle
  - isolated temporary browser profiles for default launches
  - CDP-backed page and element operations
  - page-scoped network listener lifecycle, packet capture, and extra-info merging
  - browser download mission tracking and wait/cancel flow
  - requests-backed session fetching and snapshot parsing
  - snapshot DOM node identity and relative traversal for session-backed elements
  - `WebPage` mode orchestration across browser and session
  - locator parsing
  - cookie header transfer primitives plus `cookies()` exposure for browser/session sync
  - browser download-path configuration and download waiting through CDP-backed Rust logic
  - page-scoped download path and file-conflict overrides finalized in Rust per originating tab/frame
  - page-level network blocking through CDP `Network.setBlockedURLs`
  - screenshots, PDF, DOM querying, JS execution
- Python owns:
  - compatibility-oriented wrappers
  - `ChromiumPage` convenience surface
  - object wrappers and JSON/result adaptation
  - examples and integration tests

The crate now builds as a pure Rust library by default. PyO3 bindings are enabled only with the
`python-module` feature.

## Local development

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

## Pure Rust Build

```bash
cargo check --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml --features python-module
cargo run --manifest-path rust/Cargo.toml --example webpage_modes
```

## Minimal Rust usage

```rust
use openpage_rs::{LaunchOptions, SessionOptions, WebMode, WebPage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let page = WebPage::new(
        WebMode::Driver,
        LaunchOptions::default(),
        SessionOptions::default(),
    )?;
    page.get("https://example.com")?;
    println!("{:?}", page.find("h1")?.text()?);
    page.quit()?;
    Ok(())
}
```

## Minimal Python usage

```python
from openpage import ChromiumPage

page = ChromiumPage()
page.get("https://example.com")
print(page.title)
print(page.ele("h1").text)
page.quit()
```

## Session And Mode-Switch Usage

```python
from openpage import SessionPage, WebPage

session = SessionPage()
session.get("https://example.com")
print(session.title)

page = WebPage(mode="d")
page.get("https://httpbin.org/cookies/set?token=openpage")
page.change_mode("s", copy_cookies=True)
print(page.json)
page.quit()
```

## Listener Usage

```python
from openpage import ChromiumPage

page = ChromiumPage()
listener = page.listen
listener.start(targets="/api/data", method="POST")
page.get("http://127.0.0.1:8000/")
page.ele("#trigger").click()
packet = listener.wait(timeout=5)
print(packet.method, packet.url, packet.response.status)
listener.wait_silent(timeout=5, targets_only=True)
page.quit()
```

## Interception Usage

```python
from openpage import ChromiumPage

page = ChromiumPage()
page.get("http://127.0.0.1:8000/")
page.intercept.start(targets="/api/data", method="GET")
page.ele("#trigger").click()
request = page.intercept.wait(timeout=5)
request.fulfill(
    response_code=201,
    body="intercepted",
    headers={"Content-Type": "text/plain; charset=utf-8"},
)
page.intercept.stop()
page.quit()
```

## Download Tracking Usage

```python
from openpage import ChromiumOptions, ChromiumPage

page = ChromiumPage(ChromiumOptions().set_download_path("/tmp/openpage-downloads"))
page.get("http://127.0.0.1:8000/")
page.ele("#download").click()
path = page.wait_for_download("openpage.txt", timeout=10)
mission = page.last_download()
print(path, mission.state, mission.final_path)
page.quit()
```

## Rust CLI

The CLI is implemented inside the Rust crate as `openpage_rs::cli`; it is not a separate package.
Parent CLIs can embed it with `openpage_rs::cli::run_from_args(args)`. This repository also ships a
thin debug binary:

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help
```

All user-facing CLI commands now route through the same TCP daemon execution path. There is no
separate stdio daemon mode or direct browser-execution path for the CLI surface.

Long-lived agent control over the NDJSON TCP daemon:

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --session agent
```

Daemon-backed one-command-at-a-time browser control:

```bash
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session agent --headless
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser status --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- goto https://example.com --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- title --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- js document.title --session agent
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --session agent
```

`browser list` now returns:

- `sessions` — healthy daemon-backed sessions
- `incomplete` — alive daemons with incomplete sidecars
- `cleaned` — stale sidecars cleaned during the scan

Batch multiple commands in one invocation:

```bash
OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- batch \
  "browser start https://example.com --headless" \
  "title" \
  "browser stop"

printf '%s' '[
  ["browser", "start", "https://example.com", "--headless", "--session", "agent2"],
  ["title", "--session", "agent2"],
  ["browser", "stop", "--session", "agent2"]
]' | OPENPAGE_HOME=/tmp/openpage cargo run --manifest-path rust/Cargo.toml --bin openpage -- batch
```

Diagnose the local CLI environment:

```bash
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor
```

The current CLI intentionally rejects the removed legacy surfaces:

- `serve --stdio`
- `page get`
- `page url`
- `page title`
- `page screenshot`

`doctor` reports:

- environment and daemon sidecar locations
- legacy session JSON files under `OPENPAGE_HOME/sessions` that no longer drive the active TCP CLI path
- active healthy daemon sessions
- incomplete daemon sidecars that still point to a live daemon
- stale daemon sidecars cleaned during the audit
- browser executable/config and optional live launch smoke
- machine-readable summary counts plus actionable `warn_ids` / `fail_ids` / `info_ids` / `fixable_ids`

Agent-friendly output shaping for large page payloads:

```bash
OPENPAGE_CONTENT_BOUNDARIES=1 \
OPENPAGE_MAX_OUTPUT_CHARS=2000 \
cargo run --manifest-path rust/Cargo.toml --bin openpage -- html --session agent
```

AI-first snapshot output now includes:

- `snapshot` — structured interactive-element array with stable `eN` refs
- `text` — compact text summary suitable for LLM consumption
- `refs` — ref-indexed object summary for direct follow-up actions
- `origin` / `title` — best-effort page context metadata when available

When `OPENPAGE_CONTENT_BOUNDARIES=1` is enabled and OpenPage knows the current
origin, boundary metadata now also carries that origin so models can separate
page payloads from tool output more reliably without treating the content as
trusted.

For repo-local agent usage guidance, see:

- `skills/openpage-test/references/snapshot-refs.md`
- `skills/openpage-test/references/session-management.md`
- `skills/openpage-test/references/trust-boundaries.md`

## Status

This first version is intentionally browser-first:

- Implemented:
  - `Browser`
  - `ChromiumOptions`
  - `ChromiumPage`
  - `Page`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage`
  - basic tab info
  - `get / ele / eles / run_js / click / input / clear / screenshot / pdf`
  - snapshot `s_ele / s_eles` queries for browser, session, and `WebPage`
  - snapshot root lookup plus `child / children / parent / prev / next / before / after / prevs / nexts / befores / afters`
  - snapshot node metadata `tag / inner_html / raw_text / attrs`
  - shared metadata `user_agent / status_code / cookies() / raw_data / encoding`
  - `post_json`
  - browser/page/element state checks plus first-pass wait helpers for browser-backed objects
  - browser/page/WebPage alert state and handling with `states.has_alert`, `handle_alert()`, and `wait.alert_closed()`
  - browser/session mode switching
  - current-URL cookie sync between browser and session
  - page-scoped network listener with `start / set_targets / wait / steps / wait_silent / pause / resume / clear / stop`, response body capture, and extra-info exposure when Chromium emits it
  - page/WebPage request interception with `intercept.start() / wait() / continue_request() / fail() / fulfill() / stop()`
  - browser download-path configuration plus event-driven download missions and `wait_for_download()`
  - browser-level download file-conflict handling with `rename / overwrite / skip`
  - page/WebPage download overrides through `set.download_path() / set.download_file_exists() / set.download_file_name()`
  - page/WebPage `set.upload_files()` compatibility path for browser-backed file inputs
  - page/WebPage `set.blocked_urls()` compatibility path
  - richer element-state parity for `has_rect` corner data, covered/not-covered wait helpers, and `all_downloads_done()` compatibility alias on page waits
  - browser-backed window controls for `set.window.max() / mini() / full() / normal() / size() / location() / hide() / show()` on `ChromiumPage` and driver-backed `WebPage`
  - browser-backed load-mode control for `set.load_mode.normal() / eager() / none()` plus `ChromiumOptions.load_mode` defaults on `ChromiumPage` and driver-backed `WebPage`
  - browser-backed runtime `set.user_agent()` overrides for `ChromiumPage` and driver-backed `WebPage`
  - browser-backed `set.headers() / set.local_storage() / set.session_storage() / set.auto_handle_alert()` plus session-mode `WebPage.set.headers()`
  - browser-backed `activate()` for `ChromiumPage` and driver-backed `WebPage`, with macOS process-frontmost verification for launched browser instances
  - Rust CLI subcommands for the TCP daemon plus named browser sessions

## Verification

```bash
./scripts/dev_install.sh
./scripts/run_checks.sh
```

Current integration checks cover:

- pure Rust `cargo check` and `cargo test`
- feature-gated `cargo check --features python-module`
- browser flow against `data:` pages
- session flow against `https://example.com` and `https://httpbin.org/json`
- `WebPage` browser -> session cookie sync
- `WebPage` session -> browser cookie sync
- `cookies()` access from browser, session, and `WebPage`
- page-scoped network listener capture from both `ChromiumPage` and driver-mode `WebPage`
- listener response body capture for matched browser requests
- listener response extra info exposure for matched browser requests
- listener `set_targets / pause / resume / wait_silent / steps` flow on browser-backed pages
- page/WebPage request interception for rewrite, block, and fulfill flows
- browser/page/element state/wait helpers for browser-backed objects
- alert state tracking and handling for `ChromiumPage` and driver-backed `WebPage`
- browser-backed load-mode control and navigation timing for `normal / eager / none`
- browser-backed headers/storage/auto-alert setters plus session-mode `WebPage` header setters
- event-driven download mission tracking from both `ChromiumPage` and driver-mode `WebPage`
- session-backed `raw_data` and `encoding` from Rust across `SessionPage` and `WebPage`
- local browser download flow through a configured download path and Rust-side `wait_for_download()`
- page-scoped download path, file-conflict, and download-rename overrides for both `ChromiumPage` tabs and driver-mode `WebPage`
- page/WebPage browser-backed upload-file injection through `set.upload_files()` on file inputs
- element `states.has_rect` reference-style corner data plus waiter parity
- browser-backed window bounds/state control with verified `normal / max / mini / full / size / location` flows
- browser-backed window visibility and activation control with verified `hide / show / activate` flows for launched browser instances on macOS
- Python `WebPage` thin-wrapper flow over the Rust `WebPage` core
- direct Python examples
- direct Rust `webpage_modes` example
