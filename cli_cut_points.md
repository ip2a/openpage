# OpenPage CLI Cut Points

Date: 2026-06-06

This file turns the current optimization ranking into function-level cut points.

Installed CLI context:

- `/tmp/openpage-cli-eval/bin/openpage`

Use this together with:

- `cli_optimization_roadmap.md`
- `cli_busy_state_matrix.md`
- `cli_surface_map.md`

## Project 1: Busy-session control plane / interruption semantics

### Highest-leverage cut points

1. `rust/src/cli/connection.rs::send_request_existing(...)`
2. `rust/src/cli/connection.rs::remap_existing_session_request_error(...)`
3. `rust/src/cli/connection.rs::session_target_state(...)`
4. `rust/src/cli/connection.rs::session_target_state_from_response(...)`

### Why these are the first cut

- `send_request_existing(...)` is the shared post-retry choke point for existing-session commands
- `remap_existing_session_request_error(...)` is already where transient failures are upgraded into structured busy state
- `session_target_state(...)` and `session_target_state_from_response(...)` already contain the short-timeout runtime probe logic that distinguishes:
  - present
  - missing target
  - broken target
  - unresponsive target

### Practical first move

- keep the work centralized in `connection.rs`
- extend the remap path so displaced/interrupted requests converge on one structured state story instead of leaking transport/sidecar detail or hanging too long

### Secondary support points

- `rust/src/cli/protocol.rs`
  - keep structured shell error reconstruction aligned with any richer inactive/interrupted state
- `rust/src/cli/oneshot.rs::response_result(...)`
  - only if shell-layer error reconstruction needs another small bridge

## Project 2: Forced-stop cleanup / recovery truthfulness

### Highest-leverage cut points

1. `rust/src/cli/connection.rs::shutdown_daemon(...)`
2. `rust/src/cli/connection.rs::kill_stale_daemon(...)`
3. `rust/src/cli/connection.rs::cleanup_sidecars(...)`
4. `rust/src/cli/serve.rs::run_tcp(...)` / daemon-backed session creation path

### Why these are the first cut

- `shutdown_daemon(...)` is the one place where graceful shutdown falls through into forced cleanup
- `kill_stale_daemon(...)` currently kills only the daemon pid sidecar, not the browser child
- `cleanup_sidecars(...)` currently removes only port/pid/version sidecars, which is part of why observability disappears too early
- the serve/runtime creation path is the natural place to persist a durable browser-child cleanup handle

### Practical first move

- add browser-child cleanup metadata beside the existing daemon sidecars
- write it when the daemon-backed browser/page is created
- consume it only in the forced path
- keep graceful shutdown behavior unchanged

### Secondary support points

- `rust/src/cli/oneshot.rs::start_browser(...)`
  - already does the user-facing `--replace` stop-then-start flow
  - should stay thin; recovery truth should come from the cleanup layer beneath it
- `rust/src/cli/protocol.rs::known_browser_launch_fix(...)`
  - update only after cleanup behavior is real, so profile-lock failures stop surfacing browser-path-oriented advice
- `rust/src/cli/connection.rs::daemon_unresponsive_fix(...)`
  - adjust only after the recovery path is trustworthy

## Project 3: Batch readability

### Highest-leverage cut points

1. `rust/src/cli/oneshot.rs::run_batch(...)`
2. `rust/src/cli/args.rs::BatchArgs` help text

### Why these are the first cut

- `run_batch(...)` is the whole transcript-producing loop
- it currently:
  - parses commands
  - runs them
  - prints native payloads directly
- that means the first readability fix can stay local to output shaping

### Practical first move

- add a lightweight envelope per emitted line:
  - command index
  - original argv or command text
  - payload
  - stopped/bail marker when relevant

### Secondary support points

- `README.md`
- `skills/openpage-test/references/cli-smoke.md`

Those should move only after the output shape is chosen.

## Lower-priority bucket: command discoverability / follow-up guidance polish

### Smallest cut points

1. `rust/src/cli/oneshot.rs::with_navigation_followup(...)`
2. `rust/src/cli/args.rs::HistoryGoArgs`
3. `rust/src/cli/args.rs::FrameSwitchArgs`
4. `rust/src/cli/args.rs::StorageGetArgs`
5. `rust/src/cli/args.rs` help text for click/history/frame/storage
6. `rust/src/cli/serve.rs::history.go`
7. `rust/src/cli/serve.rs::frame.switch`
8. `rust/src/cli/serve.rs` click-family `record_navigation_baseline()` call sites

### Why these are not top-three

- the flows themselves work
- the pain is:
  - discoverability
  - follow-up guidance
  - over-eager navigation token exposure in some non-navigating click cases

### Practical first moves

- enrich help text before changing runtime behavior
- only then consider narrowing which click-family operations emit `navigation_token`

## Current read

If implementation started today, the narrowest high-value first cuts would be:

1. Project 1: `connection.rs`
2. Project 2: `connection.rs` + daemon-side persisted browser-child handle
3. Project 3: `oneshot.rs::run_batch(...)`
4. Lower-priority polish: `args.rs` + `with_navigation_followup(...)`
