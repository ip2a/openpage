# Task Plan: openpage Rust Core + Python Thin API

## Goal
Build a runnable `openpage` project with `python/` and `rust/` directories, where the browser automation core is implemented in Rust and Python primarily exposes thin API wrappers over the Rust implementation.

## Deliverables
- [x] Root has `python/` and `rust/`
- [x] Rust crate builds a Python extension locally
- [x] Python package imports and uses the local Rust artifact
- [x] Core browser automation flow works end-to-end against Chromium CDP
- [x] Python API is usable and intentionally aligned with Rust API
- [x] Tests/examples pass locally
- [x] Docs explain architecture, local build, and usage

## Phases
- [x] Phase 1: Plan, repo setup, and architecture boundary
- [x] Phase 2: Scaffold Rust core, Python package, and local build flow
- [x] Phase 3: Implement browser core in Rust
- [x] Phase 4: Expose Python thin wrappers and compatibility-oriented API
- [x] Phase 5: Add tests, examples, verification scripts, and docs
- [ ] Phase 6: Audit against user requirements and finalize

## Key Questions
1. What is the smallest complete core feature set that makes `openpage` genuinely usable?
2. Which parts should live only in Rust versus thin Python compatibility sugar?
3. Which Rust CDP ecosystem pieces are stable enough to depend on directly?

## Decisions Made
- Use a persistent planning file workflow because this is a long, multi-stage build.
- Keep the target shape as `Rust core owns execution + Python owns import surface and selected compatibility wrappers`.
- Ship a browser-first first release: `Browser / ChromiumOptions / ChromiumPage / Page / Element`.
- Use `chromiumoxide` as the current Rust Chromium/CDP backbone and expose it through PyO3.
- Expand to `SessionPage` first, then move `WebPage` orchestration into Rust after the browser core is already green.
- Keep `WebPage` implemented as orchestration over Rust browser/session primitives instead of inventing a separate third transport.
- Make `pyo3` optional so the same crate can be used as a direct Rust library and as the Python extension backend.
- Treat snapshot querying as a Rust-core capability and keep Python `s_ele / s_eles` as wrappers over Rust results.
- Prioritize the snapshot DOM core next: strengthen `SessionElement` traversal before starting new browser-only subsystems like listener/download.
- Snapshot traversal MVP is now in Rust: root lookup plus `parent / children / prev / next` and node metadata are exposed through the same Python thin wrappers.
- Snapshot traversal family is now broader in Rust: `child / children / parent / prev / next / before / after / prevs / nexts / befores / afters` all execute in the Rust core and Python forwards to them.
- Snapshot metadata and shared response metadata have started moving down too: `tag / inner_html / raw_text / attrs / user_agent / WebPage.status_code` now come from the Rust core.
- `cookies()` is now part of that same shared-metadata move: browser, session, and `WebPage` expose Rust-owned cookie data and Python only adapts the result shape.
- Session response metadata has moved further down as well: `raw_data` and `encoding` now come from the Rust core and Python only forwards them.
- Browser download-path configuration now comes from Rust too: launch options and runtime setters call CDP `Browser.setDownloadBehavior`, Python only forwards the path, and download completion waiting is handled in Rust instead of Python-side polling loops.
- Browser-side listener coverage now has a Rust-owned first pass too: page-scoped request/response/failure packet capture plus `start / wait / clear / stop` live in the Rust core, and Python only exposes thin compatibility wrappers around those objects.
- Listener coverage has now moved one layer deeper as well: matched browser packets now capture response bodies in Rust before Python sees them.
- Listener extra info is now part of that same Rust-owned path too: request/response extra headers are merged in Rust, response extra metadata is exposed through PyO3, and Python only wraps those objects.
- Browser-backed wait/state helpers have started moving down too: page title/url/load checks, locator presence waits, and element state polling now execute in Rust, while Python only exposes `wait` and `states` objects.
- Browser-level wait/state coverage has moved further down as well: new-tab waiting, download begin/done waiting, and browser/page alive/headless checks now execute in Rust and Python only forwards them.
- Browser download handling has moved further down too: a Rust-owned download tracker now consumes CDP browser download events, exposes mission state, and keeps Python as a thin wrapper over mission inspection and waiting.
- `WebPage.wait` and `WebPage.states` properties are now implemented in Rust and exposed through Python thin wrappers, covering driver/session mode uniformly for alive/loading/headless/ready_state checks and new-tab/download-begin/downloads-done/url-change/title-change/load-start/doc-loaded/element-loaded waits.

## Errors Encountered
- `uvx` was not on the reduced PATH inside scripted commands; resolved by using `uv tool run maturin`.
- Direct `./scripts/*.sh` execution is unreliable on this mounted path; verified `bash scripts/*.sh` instead.
- `TargetId` requires `TargetId::new(StringLike)` and `as_ref()` instead of direct `From<&str>` / `to_string()`.
- Browser tab count is not guaranteed to start at `1`; tests now assert capability rather than a fragile initial state.
- `reqwest 0.13.3` uses the `rustls` feature name instead of `rustls-tls`.
- `SessionElement` cannot safely return references into a temporary parsed DOM; session lookups now resolve values within each method call.
- PyO3 methods were holding the interpreter lock during blocking Rust work; critical browser/session calls now use `py.detach(...)`.
- Parallel browser launches can fight over Chromium's default temp profile lock; verification now treats browser examples/tests as serial checks.
- Public `httpbin` endpoints can occasionally return a transient non-success status; verification now retries those requests in tests/examples instead of treating one external blip as a core regression.
- `run_checks.sh` originally trusted whatever Rust extension was already installed in `python/.venv`; it now rebuilds and reinstalls the local extension first so Python verification cannot pass against stale artifacts.
- The first listener implementation hit a borrow-checker conflict when draining timed-out packets; resolved by capturing the queue length before the mutable drain call.
- The first download-tracker implementation needed to keep event-driven state while preserving the existing `wait_for_download()` API; resolved by making CDP events primary and filesystem checks a compatibility fallback when Chrome does not report a final path.

## Completion Audit
- Required root layout:
  - `python/` and `rust/` exist and are the two user-facing code roots.
- Rust-core plus Python-thin-wrapper shape:
  - Rust owns browser/CDP/session/snapshot/`WebPage` execution.
  - Python imports the local Rust artifact and mostly forwards to it.
- Local build and link path:
  - `rust/` builds as pure Rust and as an optional `python-module`.
  - `python/` imports `openpage_rs` locally through the editable development flow.
- Concrete verified behavior:
  - browser flow works end to end
  - session flow works end to end
  - `WebPage` orchestration lives in Rust
  - snapshot traversal and selected metadata live in Rust
  - cookie sync plus `cookies()` / `raw_data` / `encoding` exposure now live in Rust
  - browser-backed browser/page/element state checks and first-pass wait helpers now live in Rust
  - page-scoped network listener now lives in Rust, captures response bodies and response extra info, and is reachable from both `ChromiumPage` and driver-mode `WebPage`
  - browser download-path configuration, event-driven download missions, download waiting, and local file download now live in Rust
- Still missing before the goal can be considered complete:
  - page-level element-state waits (e.g. `wait.ele_displayed/hidden/enabled/clickable`) via locator instead of element handle
  - listener interception-style controls (request/response modification, blocking)
  - richer download-manager policies (rename/skip/overwrite coordination, per-tab controls)
  - a stronger completion pass against the remaining compatibility surface

## Status
**Currently in Phase 6** - `WebPage.wait/states` parity is now green and committed. The remaining gaps are page-level element-state waits, listener interception controls, richer download policies, and the final compatibility audit. Next focus: add page-level element-state waits (`ele_displayed`, `ele_hidden`, `ele_enabled`, `ele_deleted`, `ele_clickable`) to `Page`, `WebPage`, and Python wrappers so users can wait by locator without first fetching an element handle.
