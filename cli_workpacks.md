# OpenPage CLI Workpacks

Date: 2026-06-06

This file converts the current PR sequence into short execution workpacks.

Installed CLI context:

- `/tmp/openpage-cli-eval/bin/openpage`

Use this with:

- `cli_pr_sequence.md`
- `cli_cut_points.md`
- `cli_optimization_checklists.md`

## Workpack 1

### Name

Busy-session interruption semantics

### Target outcome

- displaced or interrupted existing-session requests converge on one structured state story

### In scope

- `rust/src/cli/connection.rs`
  - `send_request_existing(...)`
  - `remap_existing_session_request_error(...)`
  - `session_target_state(...)`
- `rust/src/cli/protocol.rs`
  - only if structured shell reconstruction needs support

### Not in scope

- daemon scheduling rewrite
- faster stop latency
- browser-child cleanup

### Must-have tests

- `connection.rs`
  - post-error inactive/displaced state remaps to structured inactive shell error
- installed-binary smoke
  - busy + `--replace`
  - busy + `browser stop`
  - original in-flight request and follow-up reads converge on one high-level story

### Stop condition

- do not keep broadening once the interrupted-state story is coherent on the shell surface

## Workpack 2

### Name

Forced-stop browser-child cleanup

### Target outcome

- forced shutdown removes both daemon truth and the real browser process for the session profile

### In scope

- `rust/src/cli/connection.rs`
  - `shutdown_daemon(...)`
  - `kill_stale_daemon(...)`
  - `cleanup_sidecars(...)`
- daemon-backed browser creation path for writing durable cleanup metadata

### Not in scope

- broad doctor capability expansion
- graceful-stop refactors
- batch output

### Must-have tests

- synthetic forced cleanup test for daemon pid + browser pid
- sidecar lifecycle test for new cleanup metadata
- installed-binary smoke:
  - busy -> forced stop
  - no Chrome remains for that session profile
  - follow-up `--replace` no longer hits `SingletonLock`

### Stop condition

- do not move into guidance rewrites until the real cleanup behavior is verified

## Workpack 3

### Name

Recovery guidance truthfulness

### Target outcome

- recovery fixes point to the actual remaining action after cleanup behavior is real

### In scope

- `rust/src/cli/protocol.rs`
  - browser-launch/profile-lock guidance
- `rust/src/cli/connection.rs`
  - daemon-unresponsive and broken-target recovery text
- nearby docs/help that currently amplify stale recovery advice

### Not in scope

- new cleanup design
- unrelated doc cleanup

### Must-have tests

- protocol/context tests for updated fix text
- installed-binary repro proving profile-lock failures no longer suggest browser-path fixes first

### Stop condition

- stop once runtime behavior and guidance tell the same recovery story

## Workpack 4

### Name

Batch readability

### Target outcome

- each batch output line is self-identifying

### In scope

- `rust/src/cli/oneshot.rs::run_batch(...)`
- `rust/src/cli/args.rs::BatchArgs`
- docs after output shape is finalized

### Not in scope

- daemon protocol changes
- nested workflow semantics

### Must-have tests

- per-line correlation fields
- explicit `--bail` stop marker
- installed-binary mixed-result smoke

### Stop condition

- stop once stdout alone is enough to map line -> command in mixed-result runs

## Workpack 5

### Name

Discoverability and follow-up guidance polish

### Target outcome

- common next steps are obvious from help or command output

### In scope

- click/history/frame/storage help text
- `with_navigation_followup(...)`
- selective runtime tightening only if help alone is still insufficient

### Not in scope

- recovery correctness
- deep runtime behavior changes

### Must-have tests

- help-text assertions where practical
- targeted command-output checks for follow-up guidance

### Stop condition

- stop once the most common “what do I do next?” mistakes disappear from local use
