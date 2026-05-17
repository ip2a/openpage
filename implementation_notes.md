# openpage Implementation Notes

This file is reserved for architecture notes, verification checkpoints, and any compatibility decisions that need to survive across long editing sessions.

## Current packaging model

- Rust crate name: `openpage_rs`
- Python package name: `openpage`
- Local development flow:
  1. `uv venv python/.venv`
  2. `uv tool run maturin develop --manifest-path rust/Cargo.toml --uv`
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

## Python binding behavior

- Blocking PyO3 methods now detach from the Python interpreter while waiting on Rust-side I/O and CDP operations.
- This prevents Python-side threads from being starved while `SessionPage.get()` or `Page.goto()` are in progress.

## Verification checkpoints reached

- Rust code compiles with `cargo check`.
- PyO3 extension builds and installs into `python/.venv`.
- `import openpage_rs` succeeds.
- `import openpage` succeeds after editable install of `python/`.
- Example script opens Chrome headless and reads `Example Domain`.
- Python integration tests pass.
- `SessionPage` works against external HTML and JSON endpoints.
- `WebPage` mode switching and cookie sync pass integration tests.
- Python `WebPage` now forwards to a Rust `WebPage` core instead of keeping the mode logic in Python.
