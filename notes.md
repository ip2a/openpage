# Notes: openpage build log

## Current repository facts
- Root now contains a real `openpage` project with:
  - `rust/`
  - `python/`
  - `scripts/`
  - planning/docs files
- Git repository has been initialized locally.
- Toolchain present:
  - `rustc 1.94.1`
  - `cargo 1.94.1`
  - `Python 3.14.4`
  - `uv 0.9.24`

## Reference project observations
- Reference API shape is centered around:
  - `ChromiumPage`
  - `SessionPage`
  - `WebPage`
  - `ChromiumOptions`
  - `SessionOptions`
- Strongest Rust replacement candidates are:
  - CDP transport and connection lifecycle
  - event dispatch
  - network listener
  - download manager
- Python-specific semantics worth preserving selectively:
  - Page / Element object model
  - convenience wrappers
  - configuration ergonomics

## Constraints from user
- Root must contain `python/` and `rust/`
- Python should use locally built Rust artifacts
- Result should be directly runnable and verifiable without further user input
- Do not stop early; keep auditing completion against concrete evidence

## Working direction
- Create a Rust crate that exposes a PyO3 extension module.
- Make Python package import the extension and provide a thin API.
- Preserve key names inspired by the reference project while not pretending to fully reimplement all of DrissionPage in one pass.

## Architecture conclusions from parallel research
- `chromiumoxide` is the most pragmatic current backbone for a Rust-owned Chromium/CDP core.
- `cdp-protocol` is the right long-term direction if `openpage` later wants to own more of the protocol and transport stack.
- First release should be browser-first, not `WebPage`-first.
- `WebPage` was not the right first Rust-native boundary, but it is now implemented in Rust on top of the stabilized browser/session primitives.
- `SessionElement` is semantically a snapshot object, not a live handle.
- `WebPage` compatibility floor is mode switching plus current-context cookie sync.
- The crate can now compile as a pure Rust library without `pyo3`; Python bindings are feature-gated behind `python-module`.

## Current implemented surface
- Rust core:
  - `LaunchOptions`
  - `Browser`
  - `Page`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage`
  - locator parsing for CSS / `tag:` / `t:` / `@name=value` / `xpath:`
  - browser/session cookie header transfer primitives
  - browser-backed browser/page/element state checks and wait polling
  - page-scoped network listener with Rust-owned packet queueing, filter matching, and response body capture
  - request/response extra info exposure through the same Rust listener core
  - browser download-path configuration, mission tracking, cancel/wait support, and wait-for-download helper
- Python wrappers:
  - `ChromiumOptions`
  - `Browser`
  - `Page`
  - `ChromiumPage`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage` thin wrapper over the Rust core
  - `Listener` / `ListenerPacket` thin wrappers over the Rust listener core
  - `DownloadMission` thin wrapper over the Rust download tracker
- Verified operations:
  - launch browser
  - open page
  - read `url`, `title`, `html`
  - read browser/session `user_agent`
  - read session-backed `status_code` from `WebPage` in session mode
  - read browser/session/`WebPage` `cookies()`
  - read session-backed `raw_data` and `encoding`
  - read browser/page/element states from Rust-backed browser objects
  - wait for browser new-tab/download begin/download done from Rust
  - wait for page title/url/load changes, locator presence, and element displayed/hidden/enabled/deleted/clickable states from Rust
  - download a local file through a Rust-configured browser download path and wait for it from Rust
  - capture completed browser network packets from both `ChromiumPage` and driver-mode `WebPage`
  - capture listener response bodies for matched browser requests
  - capture listener response extra info for matched browser requests
  - capture browser download missions from both `ChromiumPage` and driver-mode `WebPage`
  - query elements
  - nested snapshot queries from browser/session/html snapshots
  - snapshot root lookup plus `child / children / parent / prev / next / before / after / prevs / nexts / befores / afters`
  - snapshot node metadata `tag / inner_html / raw_text / attrs`
  - input, click, clear
  - run JS
  - screenshot
  - PDF save API
  - tab ids / count lookup
  - session HTML fetch and JSON fetch
  - `WebPage` browser -> session cookie sync
  - `WebPage` session -> browser cookie sync
  - Python bindings detach from the interpreter during blocking Rust work

## Verified commands
- `cargo check` in `rust/`
- `cargo test` in `rust/`
- `cargo check --features python-module` in `rust/`
- `bash scripts/dev_install.sh`
- `bash scripts/run_checks.sh`
- `python/.venv/bin/python python/examples/basic_usage.py`
- `python/.venv/bin/python python/examples/webpage_modes.py`
- `cargo run --manifest-path rust/Cargo.toml --example webpage_modes`

## Next audit focus
- The snapshot DOM core now covers the main relative-navigation family in Rust.
- The next parity gap inside this area is the remaining reference-style helpers around paths, comments, and richer locator modes.
- `cookies()` has now moved into the Rust core and Python only adapts the returned objects.
- Session response metadata `raw_data` and `encoding` now lives in Rust as well.
- Basic browser download enablement and wait-for-download now lives in Rust too.
- Browser-backed wait/state has a broader Rust-owned pass now, but it still lacks the broader reference-style surface such as event-driven document-load orchestration, richer element wait variants, and stronger new-tab tracking semantics.
- Listener now has response body capture and extra-info merging in Rust, but it still lacks fuller reference-style parity such as interception controls.
- Download management now has a first Rust-owned pass as well, but it still lacks richer reference-style policies such as rename/skip/overwrite coordination and broader per-tab controls.
- The next highest-value surfaces are fuller listener/download parity, then the remaining reference-style convenience and parity helpers.

## Protocol migration audit (2026-05-29)
- Current CLI reality was originally split across three paths:
  1. `serve --stdio`
  2. `serve --port`
  3. `oneshot.rs` direct browser/session control
- `rust/src/cli/protocol.rs` is already a reasonable NDJSON protocol boundary; the problem is not schema shape, but that the CLI does not consistently route through it.
- `serve --stdio` has now been removed from active code and active docs/skills; TCP daemon is the only public daemon protocol path.
- `rust/src/cli/oneshot.rs` still performs the real work for most commands through `load_session()`, `open_page()`, and `Browser::connect()`, so the daemon is not yet the single source of execution truth.
- First migrated daemon-backed CLI commands now include:
  - `browser start/stop/status`
  - `goto`, `url`, `title`, `html`
  - `snapshot`, `screenshot`
  - `click`, `fill`, `focus`, `clear`, `submit`, `check`, `uncheck`, `text`, `attr`
- Second migrated daemon-backed CLI commands now include:
  - `scroll`
  - `back`, `forward`, `reload`, `stop-loading`
  - `right-click`, `middle-click`, `double-click`
  - `key-down`, `key-up`, `shortcut`, `input`, `type`, `type-with-interval`
  - `js`, `download`
  - `intercept start/stop/status`
  - `alert accept/dismiss/text`
  - `scroll-into-view`, `hover`, `press`, `select`, `upload`
  - `drag`, `drag-to`, `drag-to-point`
  - `active-element`
  - `wait-for-url`, `wait-for-title`, `wait-for-function`, `wait-for-text`
  - `pdf`
  - `storage get/set`
  - `cookies get/set/delete/clear`
- `rust/src/cli/connection.rs` has now absorbed more of the non-CDP `agent-browser` borrowing target:
  - daemon version mismatch restart
  - startup log capture to `OPENPAGE_HOME/daemon/<session>.log`
  - broader transient error retry classification
- AI-first ref flow is now working through the daemon path:
  - `snapshot` assigns `data-op-ref`
  - CLI `click @e1` is normalized to `[data-op-ref="e1"]`
  - verified end-to-end against a data URL page
- Remaining direct-path concentration is now much narrower:
  - `drag-in`
  - generic `wait`
  - `click-to-download`
  - `click-to-upload`
  - `click-for-new-tab`
  - `tab *`
  - `frame *`
- Best near-term borrow from `agent-browser` is the daemon infrastructure only:
  - sidecar files
  - daemon discovery
  - startup/retry/timeout handling
  - graceful shutdown / idle timeout
- We should not borrow:
  - competitor CDP transport
  - competitor action dispatch layer
  - competitor element interaction internals
