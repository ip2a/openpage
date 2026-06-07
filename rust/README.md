# OpenPage Rust workspace

Rust workspace for OpenPage.

- `crates/openpage`: pure Rust library crate published to crates.io as `openpage`.
- `apps/openpage`: non-published CLI app package. It builds the `openpage` binary and depends on the library crate.
- `bindings/python`: optional PyO3 bridge package with a minimal module scaffold.

Default verification:

```bash
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/apps/openpage/Cargo.toml --bin openpage -- --help
```
