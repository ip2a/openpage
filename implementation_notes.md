# openpage Implementation Notes

This file is reserved for architecture notes, verification checkpoints, and any compatibility decisions that need to survive across long editing sessions.

## Current packaging model

- Rust crate name: `openpage_rs`
- Python package name: `openpage`
- Rust crate default shape: pure Rust `rlib`
- PyO3 bindings: optional `python-module` feature
- Local development flow:
  1. `uv venv python/.venv`
  2. `uv tool run maturin develop --manifest-path rust/Cargo.toml --features python-module --uv`
  3. `uv pip install --python python/.venv/bin/python -e python`

Python wrappers import the compiled extension as a top-level module:

```python
import openpage_rs as _openpage_rs
```

This avoids mixed-project import-name ambiguity while keeping `import openpage` as the user-facing entrypoint.

## Browser-first scope choice

The first working implementation started by excluding:

- `SessionPage`
- `SessionElement`
- `WebPage`
- advanced listener parity
- download-manager parity

That boundary was useful for getting the browser core green first. The current state has now expanded to include:

- `SessionPage`
- `SessionElement`
- Rust-native `WebPage`
- current-URL cookie sync between browser and session

The architectural decision did not change:

- `WebPage` is implemented in Rust as orchestration over Rust browser/session primitives.
- Rust owns the execution primitives for browser and session work.
- Python owns the thin compatibility wrappers and object adaptation layer.
- Snapshot `SessionElement` nodes now carry Rust-side node identity, which made root/child/children/parent/prev/next/before/after traversal possible without moving DOM logic back into Python.
- Shared metadata access is moving down with the same rule: Python now reads `user_agent`, `status_code`, `cookies()`, `raw_data`, `encoding`, `raw_text`, and `attrs` from Rust rather than re-deriving them locally.
- Browser download configuration now follows the same Rust-first rule too: Python passes a download path into Rust, Rust applies `Browser.setDownloadBehavior` directly through CDP, and Rust owns the download-complete polling helper instead of leaving it to Python-side loops.

## Python binding behavior

- Blocking PyO3 methods now detach from the Python interpreter while waiting on Rust-side I/O and CDP operations.
- This prevents Python-side threads from being starved while `SessionPage.get()` or `Page.goto()` are in progress.

## Verification checkpoints reached

- Rust code compiles with `cargo check`.
- Rust unit tests compile and pass with `cargo test`.
- Rust example runs directly without the Python feature.
- PyO3 extension builds and installs into `python/.venv`.
- `import openpage_rs` succeeds.
- `import openpage` succeeds after editable install of `python/`.
- Example script opens Chrome headless and reads `Example Domain`.
- Python integration tests pass.
- `SessionPage` works against external HTML and JSON endpoints.
- `WebPage` mode switching and cookie sync pass integration tests.
- Python `WebPage` now forwards to a Rust `WebPage` core instead of keeping the mode logic in Python.
- Python snapshot queries now route through Rust and support nested `SessionElement` lookups.
- Snapshot traversal now covers root lookup plus `child / children / parent / prev / next / before / after / prevs / nexts / befores / afters`, `WebPage.status_code` exposes session-mode HTTP status from the shared Rust core, `cookies()` is exposed from the same Rust core across browser/session/`WebPage`, and session response metadata `raw_data` / `encoding` is now stored and exposed from Rust too.
- Browser download-path configuration, Rust-side `wait_for_download()`, and a local file-download flow are now verified from Python against the Rust core, and `run_checks.sh` rebuilds the local extension before Python checks so verifier results cannot accidentally come from stale Rust artifacts.
