# Release Matrix

OpenPage release automation is split by surface:

- Rust crate: `rust/crates/openpage`
- CLI binary: `rust/apps/openpage`
- Python package: `python/openpage` plus `rust/bindings/python`
- npm package: `npm/packages/openpage`
- npm platform packages: `npm/packages/internal/*`

Current release workflows:

- `.github/workflows/ci.yml`
- `.github/workflows/release-build.yml`
- `.github/workflows/release-publish-crates.yml`
- `.github/workflows/release-publish-npm.yml`
- `.github/workflows/release-publish-pypi.yml`
- `.github/workflows/post-release-verify.yml`
