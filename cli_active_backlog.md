# OpenPage CLI Active Backlog

Date: 2026-06-06

This file lists only the optimization work that is still active.

It intentionally excludes:

- historical landed slices
- exploratory notes
- lower-priority polish unless it is still worth opening as a separate issue

Installed CLI context:

- `/tmp/openpage-cli-eval/bin/openpage`

## Issue 1

### Title

CLI: converge busy and displaced session requests on one structured state story

### Priority

P0

### Why it is active

- busy-session command behavior is still the broadest runtime pain
- interruption semantics are still not fully settled across command classes

### First cut

- `rust/src/cli/connection.rs`

### First acceptance read

- interrupted or displaced requests no longer leak low-level transport/sidecar detail
- shell-facing commands tell one coherent session-state story

## Issue 2

### Title

CLI: make forced-stop cleanup kill the browser child and preserve recovery observability

### Priority

P1

### Why it is active

- repeated local busy + `--replace` runs still reproduce the same profile-lock failure
- forced cleanup can erase daemon/session truth before real cleanup is complete

### First cut

- `rust/src/cli/connection.rs`
- daemon-backed browser/session creation path

### First acceptance read

- after forced cleanup, no Chrome remains for that session profile
- inventory/doctor do not go blind before recovery is actually complete

## Issue 3

### Title

CLI: align recovery guidance with real forced-stop behavior

### Priority

P1/P2 boundary

### Why it is active

- profile-lock recovery failures still surface browser-path-oriented advice
- current fix text still overestimates how useful logs and `--replace` are before cleanup is fixed

### Dependency

- should follow Issue 2, not precede it

### First cut

- `rust/src/cli/protocol.rs`
- `rust/src/cli/connection.rs`

### First acceptance read

- recovery guidance points at the actual remaining action

## Issue 4

### Title

CLI: make `batch` output readable for mixed-result runs

### Priority

P2

### Why it is active

- mixed-result `batch --bail` runs are still hard to read from stdout alone

### First cut

- `rust/src/cli/oneshot.rs::run_batch(...)`

### First acceptance read

- each output line is self-identifying
- `--bail` clearly marks the stopping command

## Issue 5

### Title

CLI: polish command discoverability and follow-up guidance

### Priority

P3

### Why it is active

- local flows work, but users still have to guess some normal next steps

### Examples

- click/navigation-token follow-up guidance
- history-go follow-up guidance
- frame-switch reset discoverability
- storage-get argument discoverability

### First cut

- `rust/src/cli/args.rs`
- `rust/src/cli/oneshot.rs::with_navigation_followup(...)`

### First acceptance read

- common “what do I do next?” mistakes become rare in local use
