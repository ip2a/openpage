# CI

CI should verify each surface without changing the public product name:

- Rust workspace checks.
- CLI binary smoke tests.
- Python import and wrapper tests.
- npm package assembly and smoke tests.
- Release metadata sync.

Scripts are grouped by role:

- `scripts/dev`
- `scripts/build`
- `scripts/test`
- `scripts/release`
