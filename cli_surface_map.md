# OpenPage CLI Surface Map

Date: 2026-06-06

This file maps the current optimization projects to the user-facing surfaces they touch.

Installed CLI context:

- `/tmp/openpage-cli-eval/bin/openpage`

Purpose:

- show why the top projects matter beyond one runtime repro
- capture how widely each issue is reflected in help text, fix text, docs, and shared execution paths

## Project 1: Busy-session control plane / interruption semantics

Shared execution breadth:

- `rust/src/cli/oneshot.rs` currently has `202` `rpc_webpage(...)` call sites
- those calls converge through the same request path in `connection.rs`

User-facing surfaces:

- `browser status`
- `browser list`
- `browser logs`
- ordinary session commands such as `title`, `snapshot`, `goto`
- shell error payloads reconstructed in `rust/src/cli/protocol.rs`

Why the breadth matters:

- this is not one command being awkward
- it is the shared contract for most session-backed commands

## Project 2: Forced-stop cleanup / recovery truthfulness

Observed surface count:

- `--replace` appears in `19` matched lines across:
  - CLI help / args
  - fix text
  - repo-local smoke and session-management docs

Key user-facing files:

- `rust/src/cli/args.rs`
- `rust/src/cli/connection.rs`
- `rust/src/cli/protocol.rs`
- `skills/openpage-test/references/session-management.md`
- `skills/openpage-test/references/cli-smoke.md`

Why the breadth matters:

- recovery guidance is repeated in multiple layers
- when `--replace` or post-stop recovery is not trustworthy, the CLI is wrong in more than one place at once

## Project 3: Batch readability

Observed surface count:

- explicit batch-output wording matched only `3` lines in the searched surfaces

Key user-facing files:

- `rust/src/cli/args.rs`
- `rust/src/cli/mod.rs`

Why the breadth still matters:

- this is a smaller surface than Projects 1 and 2
- but the runtime output itself is the product surface, and it is currently hard to scan in mixed-result runs

## Lower-priority bucket: discoverability / follow-up guidance polish

Observed surface count:

- `wait-for-navigation` / `navigation_token` matched `48` lines across the searched surfaces
- `browser logs --session` / `daemon_unresponsive` / `doctor --quick --fix` matched `75` lines across the searched surfaces

Interpretation:

- this bucket is broad in references, but not broad in runtime failure severity
- most of the pain is:
  - when to follow up with `wait-for-navigation`
  - when logs are actually useful
  - which reset/switch targets are accepted
  - which arguments are intuitive on first use

## Current read

The surface map supports the current ranking:

1. Project 1 has the broadest shared runtime reach.
2. Project 2 has the sharpest repeated recovery-contract mismatch across help, fix text, and docs.
3. Project 3 has a smaller surface, but the transcript itself is still poor enough to merit its own project.
4. Discoverability polish is real, but it remains below the runtime-correctness and recovery-truthfulness work.
