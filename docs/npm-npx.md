# npm and npx

The user-visible npm package is `openpage`.

`npm/packages/openpage` provides the `openpage` bin entry and resolves the
current platform binary from internal optional dependencies.

Platform-specific packages live under `npm/packages/internal` and are not
intended to be user-facing product names.
