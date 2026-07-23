# Unified `config.toml` Refactor (2026-06-01)

## Goal
Replace ini-based active CLI runtime defaults with one stable TOML-based config chain.

## Effective precedence
1. CLI request/flags
2. Environment variables
3. Workspace config: `./.openpage/config.toml`
4. User config: `OPENPAGE_HOME/config.toml` (default `~/.openpage/config.toml`)
5. Built-in defaults

## Implemented
- Added `src/config.rs`
  - loads/merges TOML config layers
  - tracks browser executable source (`default`, `user config.toml`, `workspace config.toml`, `OPENPAGE_BROWSER_PATH`)
  - applies env overrides
  - provides cross-platform browser candidate discovery
- Wired active CLI runtime to unified config:
  - `src/cli/serve.rs`: daemon startup now starts from `load_resolved_config()`
  - `src/cli/doctor.rs`: config check + messages now come from TOML chain
  - `src/cli/mod.rs`: the CLI reads and writes the current TOML configuration directly
- Updated docs/help references from ini to TOML for touched surfaces.

## Notable verification
- `cargo check` passes.
- `doctor --quick` now reports resolved config source and TOML paths.
- Smoke checks with temporary homes/workspaces confirm source precedence:
  - user-only => `source=user config.toml`
  - user + workspace => `source=workspace config.toml`
  - + env override => `source=OPENPAGE_BROWSER_PATH`

## Known unrelated blocker
- `cargo test` currently fails due a pre-existing test compile error in `src/download.rs`, unrelated to this refactor.
