# Install And Build

## Scope

This repository is Rust-first. Python is a thin wrapper layer and is not required to prove the Rust CLI works.

## Prerequisites

- Run commands from the repository root.
- Have `cargo` available.
- Have Chrome or Chromium installed.
- Have outbound network access if you want to run the Baidu smoke tests.
- Use `uv` only if you need the optional Python wrapper install.

## Rust-Only Verification

Run these first:

```bash
cargo check --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick --fix
cargo test --manifest-path rust/Cargo.toml
cargo check --manifest-path rust/Cargo.toml --features python-module
cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor
cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help
```

Interpretation:

- `doctor --quick` is the cheap config/environment gate.
- `doctor --quick --fix` is the deterministic cleanup path for legacy session JSON residue from the removed one-shot CLI path.
- full `doctor` adds a live headless browser launch smoke.
- if full `doctor` fails with a configured `browser_path` not found, fix that before blaming the CLI transport path.
- the repo-local smoke scripts also accept `OPENPAGE_BROWSER_PATH` and will try common PATH and macOS app-bundle locations automatically.

If you want a built binary instead of `cargo run`:

```bash
cargo build --manifest-path rust/Cargo.toml --bin openpage
bash ./scripts/smoke_test.sh rust/target/debug/openpage
```

## Full Local Install With Thin Python Wrapper

Only do this after the Rust path passes:

```bash
bash ./scripts/dev_install.sh
```

That script:

- creates `python/.venv`
- builds the PyO3 module with `maturin develop --manifest-path rust/Cargo.toml --features python-module --uv`
- installs `python/` in editable mode

## Full Repo Checks

If you want the repository's full validation path:

```bash
bash ./scripts/run_checks.sh
```

This includes Rust checks, Python wrapper install, Python tests/examples, and the Rust example program.

## Python Wrapper Verification

Python should stay secondary. Run it only after Rust passes:

```bash
python/.venv/bin/python -m unittest discover -s python/tests -v
python/.venv/bin/python python/examples/basic_usage.py
python/.venv/bin/python python/examples/webpage_modes.py
```

Interpretation:

- Rust passes and Python passes: repo is healthy end-to-end.
- Rust passes and Python fails: wrapper or packaging integration needs work.
- Rust fails first: do not spend time on Python until the Rust issue is fixed.
