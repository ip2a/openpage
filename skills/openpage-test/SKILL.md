---
name: openpage-test
description: Use when verifying or debugging the local OpenPage Rust build, running CLI/browser smoke tests, checking screenshots, comparing raw TCP daemon control with named-session CLI commands, or confirming the thin Python wrapper after Rust passes.
---

# OpenPage Test

## Overview

This skill is the repo-local testing playbook for OpenPage. It treats the Rust crate as the source of truth, keeps Python optional, and focuses on commands that were re-verified against this repository.

## When To Use

- You need to install or build OpenPage locally.
- You need to verify the Rust CLI can launch and control a browser without Python.
- You need to smoke-test the TCP daemon path or named-session CLI commands.
- You need to confirm screenshots are real page renders instead of trusting file existence alone.
- You need to explain whether a failure is in Rust core, the TCP daemon path, the CLI wrapper layer, or Python integration.

## Workflow

1. If the task is local setup or Python integration, read `references/install.md`.
2. If the task is browser control or screenshot verification, read `references/cli-smoke.md`.
3. Prefer this verification order:
   - `cargo check --manifest-path rust/Cargo.toml`
   - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
   - `cargo test --manifest-path rust/Cargo.toml`
   - `cargo check --manifest-path rust/Cargo.toml --features python-module`
   - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor`
   - `bash skills/openpage-test/scripts/serve_baidu_smoke.sh`
   - `bash skills/openpage-test/scripts/named_session_baidu_smoke.sh`
4. If any compile step fails, stop there and report the compiler error before attempting runtime smoke.
5. Treat the TCP daemon path as the main agent-control path.
6. Treat named-session CLI commands as a higher-level wrapper over the same TCP daemon execution path. If raw TCP and named-session CLI disagree, investigate the CLI wrapper layer first.
7. Never trust screenshot existence alone. Always run file checks and visually inspect the image.
8. Use `doctor` results to separate local browser/config problems from TCP daemon protocol regressions before blaming the CLI transport path.

## Resources

- `references/install.md`
  - Local build, Rust-only verification, optional Python wrapper install, and repo scripts.
- `references/cli-smoke.md`
  - Exact smoke-test commands, expected outcomes, and current failure interpretation, including `doctor`.
- `scripts/serve_baidu_smoke.sh`
  - Deterministic TCP daemon smoke test that opens Baidu and saves a screenshot.
- `scripts/named_session_baidu_smoke.sh`
  - Deterministic named-session CLI smoke test through the same TCP daemon-backed execution path.

## Decision Rules

- Use repo scripts `./scripts/dev_install.sh` and `./scripts/run_checks.sh` only when you want the full repo workflow. They are not required for Rust-only CLI verification.
- If raw TCP daemon control and named-session CLI disagree, treat that as a CLI-wrapper regression first.
- If a screenshot is blank, white, or obviously not the target page, the run failed even if the CLI returned success.
- If Rust passes and Python fails, describe that as a wrapper/integration problem instead of a Rust-core failure unless the Python path reproduces a Rust-side defect.

## Expected Reporting

When using this skill, report:

- the exact commands run
- pass/fail per step
- screenshot output paths
- whether screenshots were visually checked
- whether a failure is Rust-core, TCP daemon, CLI-wrapper, or Python-wrapper specific
