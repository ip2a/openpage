# CLI

The CLI is built by `rust/apps/openpage` and exposes the `openpage` binary.
Command implementations live under `rust/apps/openpage/src/commands`, while
`rust/apps/openpage/src/cli.rs` keeps the top-level argument parsing and mode
dispatch boundary.
