# OpenPage CLI Optimization Checklists

Date: 2026-06-06

This file is the execution companion to `cli_optimization_projects.md`.

Issue companion:

- `cli_optimization_issue_cards.md`

Current usage note:

- Slice A and Slice B below are validated historical slices on the current tree
- the current active execution queue is:
  1. forced-stop browser-child cleanup
  2. recovery guidance truthfulness
  3. batch readability
  4. discoverability polish
- for the live ordering and cut points, treat `cli_workpacks.md` and `cli_pr_sequence.md` as authoritative

It is intentionally narrow:

- exact edit points
- first tests to add
- wording surfaces that must move together
- explicit non-goals for the first slice

## Slice A: Busy Truthfulness

Goal:

- ordinary commands on a busy session should stop surfacing only low-level transient transport noise
- shell callers should get the same busy/unresponsive truth that `browser status` and `browser list` already know

### First tests to add

1. `rust/src/cli/connection.rs`
   - add a test for:
     - retry exhaustion on an existing session request
     - runtime probe returns `SessionTargetState::Unresponsive`
     - final error is remapped away from low-level transient transport noise
2. `rust/src/cli/protocol.rs`
   - add a round-trip test for:
     - `kind="browser_operation"`
     - `session="review"`
     - `state="incomplete"`
     - `reasons=["daemon_unresponsive"]`
     - busy-session fix text
3. `rust/src/cli/oneshot.rs`
   - add a shell-payload test for:
     - structured busy error stays structured through `response_result(...)`

### Code edit checklist

1. `rust/src/cli/connection.rs`
   - inspect `send_request_existing(...)`
   - add a thin remap path around `send_request_with_retry(...)`
   - after transient retry exhaustion:
     - read `daemon_status(session)?`
     - read `session_target_state(&status)?`
     - if `Unresponsive`, return a structured browser/session error
2. `rust/src/cli/protocol.rs`
   - extend `openpage_error_from_structured_context(...)`
   - extend `openpage_error_context(...)`
   - make sure structured busy state round-trips back to:
     - `error.kind="browser_operation"`
     - `error.session`
     - `error.state="incomplete"`
     - `error.reasons=["daemon_unresponsive"]`
     - `error.fix`

### Public behavior to preserve in the first pass

- do not add a new public error kind yet
- keep first-pass `error.kind="browser_operation"`
- use `state/reasons` to express busy/unresponsive

### First-pass non-goals

- do not rewrite the daemon accept loop
- do not attempt full async navigation here
- do not promise faster `browser stop` latency yet

## Slice B: Replace Contract Truthfulness

Goal:

- make `--replace` either real or no longer publicly promised

### First tests to add

1. `rust/src/cli/oneshot.rs`
   - add a test that proves:
     - active-session `--replace` does not simply fall through to `already_running=true`
2. runtime smoke
   - active named session
   - run `browser start --replace`
   - verify a healthy runtime is re-established
3. docs/help wording checks
   - remove wording that implies fresh-state reset unless that behavior is truly implemented

### Code edit checklist

1. `rust/src/cli/oneshot.rs`
   - update `start_browser(args)`
   - if `args.replace`:
     - call `stop_browser_session(&args.session, true)?`
     - then continue through the existing create/start path
2. `rust/src/cli/args.rs`
   - rewrite `replace` help text so it means runtime restart, not state reset
3. `rust/src/cli/protocol.rs`
   - update fix text that currently recommends `--replace`
   - make that text consistent with the preserved-profile semantics
4. `rust/src/cli/connection.rs`
   - update fix text for:
     - `unknown target`
     - `broken_target`
     - `daemon_unresponsive`
   - remove language that implies clean-state reset
5. docs
   - `README.md`
   - `skills/openpage-test/references/session-management.md`
   - `skills/openpage-test/references/cli-smoke.md`

### Public behavior to preserve in the first pass

- named session profile continuity
- same `OPENPAGE_HOME/profiles/<session>` mapping
- `--replace` means:
  - restart this named session runtime
  - preserve browser state unless users explicitly manage profile/state themselves

### First-pass non-goals

- do not implement fresh-state reset semantics yet
- do not solve orphan Chrome on forced cleanup in this slice
- do not broaden `doctor --quick --fix` in this slice

## Doc Cleanup Follow-up

These are not top-level optimization projects, but they should be cleaned as part of Slice B:

1. `skills/openpage-test/references/session-management.md`
   - current wording still implies explicit `--user-data-dir` is the main path for profile reuse
   - current runtime truth is that named sessions already default to persistent profile dirs
2. any fix text saying or implying:
   - `--replace` means "clean restart"
   - `--replace` means browser-state reset

## Recommended order

1. land Slice A tests
2. land Slice A implementation
3. land Slice B tests and wording updates
4. land Slice B implementation
5. only then evaluate whether recovery foundation or async navigation should be the next stream

## Active follow-up slices on current tree

### Slice C: Recovery guidance truthfulness

Goal:

- once forced cleanup is real, recovery text should point at the actual remaining action

First tests to add:

1. `rust/src/cli/protocol.rs`
   - fix-text assertions for profile-lock recovery after forced-cleanup defects
2. installed-binary smoke
   - repro a profile-lock recovery failure before/after cleanup fix
   - verify browser-path advice is no longer shown first

Code edit checklist:

1. `rust/src/cli/protocol.rs`
   - browser-launch/profile-lock guidance
2. `rust/src/cli/connection.rs`
   - daemon-unresponsive / broken-target recovery wording

### Slice D: Discoverability polish

Goal:

- make the common next step obvious from help or output

First tests to add:

1. `rust/src/cli/args.rs`
   - help-text assertions where practical
2. installed-binary smoke
   - click/history/frame/storage first-use flows
   - verify fewer follow-up missteps

Code edit checklist:

1. `rust/src/cli/oneshot.rs`
   - `with_navigation_followup(...)`
2. `rust/src/cli/args.rs`
   - click/history/frame/storage help text

## Current status

### Slice A: Busy Truthfulness

- [x] `connection.rs` remap after retry exhaustion
- [x] `protocol.rs` structured busy-state round-trip
- [x] `oneshot.rs` shell-layer reconstruction test
- [x] installed-binary repro for `title` / `snapshot` during in-flight navigation
- [ ] follow-up phases:
  - stop latency
  - true interruptibility
  - daemon scheduling/concurrency changes

### Slice B: Replace Contract Truthfulness

- [x] `start_browser(args)` now stops the named session before relaunch when `args.replace`
- [x] help/reference wording now says restart/runtime truth instead of implying fresh-state reset
- [x] installed-binary healthy-session repro:
  - `--replace` no longer returns `already_running=true`
  - profile continuity survives the restart
- [ ] remaining follow-up:
  - busy-session recovery still depends on forced-stop browser-child cleanup
  - profile-lock relaunch failures still return misleading browser-path `browser_launch` fix text
  - after forced cleanup, sidecar removal currently makes `browser list` / `doctor --quick --fix` lose visibility into the remaining orphan-browser problem
  - review whether busy-session fix text should keep pointing first to `--replace` before that cleanup lands

### Forced-stop cleanup boundary

- [x] normal `browser stop` control run:
  - no orphan Chrome remained
- [x] forced-stop recovery run:
  - daemon/session state cleaned
  - Chrome still survived for the session profile
- [ ] next work:
  - keep cleanup work focused on the forced path rather than broad stop-path churn
  - persist browser child pid or equivalent durable cleanup handle for daemon-backed sessions
  - extend forced cleanup to kill browser child as well as daemon pid
  - re-verify profile-lock relaunch behavior after that cleanup lands
  - preserve the current one-browser-per-session assumption only for the active daemon-backed CLI surface; revisit if protocol ownership broadens

### Project 2: first implementation slice

Goal:

- make forced cleanup complete for the current daemon-backed session model without changing normal graceful stop behavior

#### First tests to add

1. `rust/src/cli/connection.rs`
   - sidecar lifecycle test:
     - cleanup removes the new browser-pid sidecar alongside port/pid/version
   - forced cleanup test:
     - synthetic daemon pid + synthetic browser pid
     - `shutdown_daemon(...)` forced path kills both
     - all sidecars are removed
2. `rust/src/cli/serve.rs`
   - session create path test if practical:
     - when a driver-mode webpage is created for the session target, browser pid sidecar is written
   - if direct serve-unit coverage is awkward, cover this via installed-binary smoke instead
3. installed-binary smoke
   - busy session -> forced stop
   - verify:
     - `browser list` becomes empty
     - no Chrome process remains for that session profile
   - then rerun:
     - `browser start --session <name> --replace ...`
     - verify no `SingletonLock` failure remains

#### Code edit checklist

1. `rust/src/cli/connection.rs`
   - add browser-pid sidecar helpers near existing port/pid/version helpers
   - extend `cleanup_sidecars(...)`
   - extend forced cleanup to terminate browser child as well as daemon pid
2. `rust/src/cli/serve.rs`
   - write the browser-pid sidecar when the daemon-backed session target launches its browser-backed `WebPage`
3. docs/fix follow-up only after behavior is real
   - `rust/src/cli/protocol.rs`
   - `rust/src/cli/connection.rs`
   - remove browser-path-oriented guidance for profile-lock recovery failures once the cleanup path is fixed

#### First-pass risks to watch

- stale browser-pid sidecar pointing at a reused OS pid
- writing the sidecar too early or too late relative to launch failures
- accidentally treating future multi-browser-per-session protocol extensions as if they already existed

### Batch ranking confirmation

- [x] mixed-result batch run confirmed:
  - raw NDJSON lines only
  - no command index
  - no original argv echo
- [x] `--bail` run confirmed:
  - output simply stops after the failing command
  - no explicit stopping-command marker is added
- [x] startup-observation recheck did not reveal a stronger third project

### Project 3: first implementation slice

Goal:

- keep the existing execution semantics and NDJSON stream shape, but make each output line self-identifying for humans and automation

#### First tests to add

1. `rust/src/cli/oneshot.rs`
   - mixed-result batch payload test:
     - each emitted line carries command correlation fields
       - command index
       - original argv or command text
   - `--bail` payload test:
     - failing/stopping line makes it obvious that execution stopped there
2. runtime smoke
   - mixed success/failure batch run
   - verify stdout alone makes it obvious:
     - which command produced each line
     - which command triggered `--bail`

#### Code edit checklist

1. `rust/src/cli/oneshot.rs`
   - wrap each per-command result in a lightweight batch envelope before printing
   - preserve the underlying command payload under a stable field rather than flattening it into ad hoc text
2. `rust/src/cli/args.rs`
   - update batch help text so the documented output shape matches the new correlation fields
3. docs
   - `README.md`
   - `skills/openpage-test/references/cli-smoke.md`

#### First-pass risks to watch

- breaking downstream consumers that assume batch output lines are exactly the raw command payloads
- adding correlation data in a way that duplicates too much large payload text
- inventing a wrapper shape that is harder to scan than the current plain NDJSON

### Busy/Interruption follow-up

- [ ] when `--replace` interrupts an in-flight navigation:
  - ordinary follow-up reads now fail as `inactive`
  - but the original in-flight `goto` has previously been seen leaking low-level `io error: daemon port not found`
- [ ] decide whether displaced commands should converge on:
  - `inactive`
  - a richer interrupted/replaced session state
  - or another explicit structured cancellation story

### Project 1: next implementation slice after busy truthfulness

Goal:

- stop displaced in-flight commands from leaking back out as low-level transport/sidecar errors once session state has already converged elsewhere

#### First tests to add

1. `rust/src/cli/connection.rs`
   - remap test for:
     - transient error during existing-session request
     - follow-up `daemon_status(session)` shows the session is no longer alive/active
     - final shell error becomes the same structured inactive session story instead of low-level `io error: daemon port not found`
2. installed-binary smoke
   - busy session
   - trigger `browser start --replace` or equivalent displacement
   - verify:
     - `title` / `snapshot` and the original in-flight `goto` now converge on one high-level state story

#### Code edit checklist

1. `rust/src/cli/connection.rs`
   - extend `remap_existing_session_request_error(...)`
   - today it only upgrades transient exhaustion into busy/unresponsive when the daemon still looks alive+ready
   - next slice should also inspect the post-error state where the session has become inactive/displaced
2. `rust/src/cli/protocol.rs`
   - reuse the existing structured inactive session shape where possible
   - avoid introducing a new public error kind unless the inactive story proves insufficient

#### First-pass risks to watch

- collapsing genuinely retryable transport blips into inactive too aggressively
- conflating “session replaced/stopped during request” with “user typo / never-started inactive session”
- overfitting the inactive story if a later explicit interrupted/replaced state becomes necessary
