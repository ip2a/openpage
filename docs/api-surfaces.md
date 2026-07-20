# API Surfaces

## Stable Product Name

All public entry points use `openpage`.

## Surfaces

| Surface | Entry | Location |
| --- | --- | --- |
| Rust library | `openpage` crate | `rust/crates/openpage` |
| CLI | `openpage ...` | `rust/apps/openpage` |
| MCP | `openpage` / `openpage mcp` | `rust/apps/openpage` and future core modules |
| TCP daemon | `openpage serve` | `rust/apps/openpage` |
| Python | `import openpage` | `python/openpage` |
| npm/npx | `npx openpage` | `npm/packages/openpage` |

Implementation-only packages may carry suffixes. Public commands and package
names should not.
