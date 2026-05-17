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

The first working implementation intentionally excludes:

- `SessionPage`
- `SessionElement`
- `WebPage`
- advanced listener parity
- download-manager parity

Reason:

- The highest-value runnable path is `page.get() -> page.ele() -> element.click()/input()/text`.
- `WebPage` is not a primitive core object; it is a higher-level compatibility and orchestration layer.

## Verification checkpoints reached

- Rust code compiles with `cargo check`.
- PyO3 extension builds and installs into `python/.venv`.
- `import openpage_rs` succeeds.
- `import openpage` succeeds after editable install of `python/`.
- Example script opens Chrome headless and reads `Example Domain`.
- Python integration tests pass.
