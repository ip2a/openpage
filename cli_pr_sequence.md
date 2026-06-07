# OpenPage CLI PR Sequence

Date: 2026-06-06

This file turns the current optimization ranking into a practical implementation order.

Installed CLI context:

- `/tmp/openpage-cli-eval/bin/openpage`

Use this with:

- `cli_optimization_roadmap.md`
- `cli_cut_points.md`
- `cli_optimization_issue_cards.md`
- `cli_optimization_checklists.md`

## Ordering rule

The sequence below is ordered by:

1. user pain
2. dependency between fixes
3. blast radius
4. ease of verification on the installed CLI

## PR 1

### Title

Busy-session interruption semantics: converge displaced requests on one structured state story

### Why this comes first

- it improves the shared error contract for the largest surface area first
- it does not require browser-child persistence or cleanup design
- it reduces user confusion even before deeper recovery work lands

### Core cut points

- `rust/src/cli/connection.rs`
  - `send_request_existing(...)`
  - `remap_existing_session_request_error(...)`
  - `session_target_state(...)`
- `rust/src/cli/protocol.rs`
  - only as needed for structured shell reconstruction

### Success read

- busy or displaced commands stop leaking low-level transport/sidecar detail
- interrupted follow-up reads and the original in-flight request tell one high-level session-state story

### Why not start with Project 2

- Project 2 is blocked on a cleanup-handle design
- Project 1 can ship user-visible truthfulness sooner without that dependency

## PR 2

### Title

Forced-stop recovery foundation: persist browser-child cleanup metadata and kill the browser child on forced shutdown

### Why this comes second

- this is the true blocker for reliable busy-session recovery
- it depends on a tighter cleanup design, so it is slightly heavier than PR 1
- once it lands, the existing `--replace` flow can finally become trustworthy in the hardest case

### Core cut points

- `rust/src/cli/connection.rs`
  - `shutdown_daemon(...)`
  - `kill_stale_daemon(...)`
  - `cleanup_sidecars(...)`
- `rust/src/cli/serve.rs`
  - daemon-backed browser/session creation path for writing the cleanup handle

### Success read

- forced stop leaves no Chrome process alive for that session profile
- `browser start --replace` no longer fails on `SingletonLock` after a forced busy-session stop
- recovery observability is preserved until cleanup is actually complete

## PR 3

### Title

Recovery guidance truthfulness: remove browser-path-oriented advice from profile-lock recovery failures and align fix text with the now-real cleanup path

### Why this comes third

- fix text should only be rewritten after recovery behavior is real
- otherwise the CLI risks replacing one misleading story with another

### Core cut points

- `rust/src/cli/protocol.rs`
  - `known_browser_launch_fix(...)`
- `rust/src/cli/connection.rs`
  - `daemon_unresponsive_fix(...)`
  - related incomplete/broken-target recovery text
- docs/help nearby recovery wording

### Success read

- recovery failures point users at the actual remaining action
- browser-path advice is not shown for profile-lock cleanup defects

## PR 4

### Title

Batch readability: add per-line command correlation and explicit bail stop markers

### Why this comes fourth

- it is valuable, but not on the critical recovery path
- it is small, local, and easy to verify
- it does not need to wait on deeper daemon/runtime work

### Core cut points

- `rust/src/cli/oneshot.rs::run_batch(...)`
- `rust/src/cli/args.rs::BatchArgs`
- docs after output shape is chosen

### Success read

- stdout alone makes mixed-result batch runs readable
- `--bail` makes the stopping command explicit

## PR 5

### Title

Discoverability polish: improve follow-up guidance and small CLI help gaps

### Why this comes last

- the flows work today
- the pain is real but mostly ergonomic
- it should not compete with correctness and recovery work

### Core cut points

- `rust/src/cli/oneshot.rs::with_navigation_followup(...)`
- `rust/src/cli/args.rs`
  - click/history/frame/storage help text
- selectively narrower runtime changes only if the help updates are still insufficient

### Success read

- users can infer the normal next step from command output/help without guesswork
- common small mistakes become rarer without changing the runtime contract too aggressively

## Current recommendation

If work started immediately, the best sequence is:

1. PR 1: busy-session interruption semantics
2. PR 2: forced-stop browser-child cleanup
3. PR 3: recovery guidance truthfulness
4. PR 4: batch readability
5. PR 5: discoverability polish

This preserves momentum:

- first fix the shared story
- then fix the real recovery blocker
- then update the guidance to match reality
- then clean up lower-risk UX work
