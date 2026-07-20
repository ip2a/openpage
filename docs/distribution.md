# Distribution

OpenPage keeps one user-visible name across distribution channels.

## User-Visible Names

- `openpage` binary
- `openpage` Rust crate
- `openpage` Python package
- `openpage` npm package

## Internal Platform Packages

The npm platform packages live under `npm/packages/internal`. Their suffixes are
implementation details used by install-time resolution:

- `openpage-bin-darwin-arm64`
- `openpage-bin-darwin-x64`
- `openpage-bin-linux-x64-gnu`
- `openpage-bin-linux-arm64-gnu`
- `openpage-bin-win32-x64-msvc`

Users install and run only `openpage`.
