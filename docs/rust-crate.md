# Rust Crate

The public Rust crate is `openpage` at `rust/crates/openpage`.

It should remain the source of truth for reusable browser automation behavior.
CLI, MCP, Python, and npm surfaces should adapt this crate rather than duplicate
browser logic.
