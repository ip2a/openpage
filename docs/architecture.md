# OpenPage Architecture

OpenPage is a Rust-first browser automation project with one user-facing name:
`openpage`.

The Rust library crate is the source of truth for browser, page, element,
session, network, download, wait, snapshot, and frame behavior. Other surfaces
adapt that same capability instead of reimplementing it.

## Public Surfaces

- Rust crate: `openpage`
- Binary command: `openpage`
- npm entry: `npx openpage`
- Python package: `openpage`
- Python command entry: `python -m openpage`
- MCP mode: `openpage` or `openpage mcp`, not a separate product name

## Repository Boundaries

- `rust/crates/openpage`: Rust core library and reusable internals.
- `rust/apps/openpage`: the only user-visible binary package.
- `rust/bindings/python`: PyO3 bridge for the Python package.
- `python/openpage`: Python-friendly wrappers.
- `npm/packages/openpage`: the only user-visible npm package.
- `npm/packages/internal`: platform binary packages used by npm distribution.
- `examples`: examples grouped by usage mode.
- `tests`: cross-surface tests grouped by target surface.
- `scripts`: developer, build, test, and release helpers.

Platform suffixes are allowed in internal artifacts and package folders only.
They must not become user-visible command names.
