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

## Errors Encountered
- `uvx` was not on the reduced PATH inside scripted commands; resolved by using `uv tool run maturin`.
- Direct `./scripts/*.sh` execution is unreliable on this mounted path; verified `bash scripts/*.sh` instead.
- `TargetId` requires `TargetId::new(StringLike)` and `as_ref()` instead of direct `From<&str>` / `to_string()`.
- Browser tab count is not guaranteed to start at `1`; tests now assert capability rather than a fragile initial state.
- `reqwest 0.13.3` uses the `rustls` feature name instead of `rustls-tls`.
- `SessionElement` cannot safely return references into a temporary parsed DOM; session lookups now resolve values within each method call.
- PyO3 methods were holding the interpreter lock during blocking Rust work; critical browser/session calls now use `py.detach(...)`.
- Parallel browser launches can fight over Chromium's default temp profile lock; verification now treats browser examples/tests as serial checks.

## Status
**Currently in Phase 6** - auditing the expanded browser + session implementation now that `WebPage` also lives in Rust, and identifying the remaining parity gaps beyond the green core flow.
