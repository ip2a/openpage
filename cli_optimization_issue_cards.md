# OpenPage CLI Optimization Issue Cards

Date: 2026-06-06

This file converts the current CLI optimization shortlist into issue-ready cards.

Use it when opening issues, planning PRs, or sequencing the next implementation stream.

Related docs:

- `cli_optimization_projects.md`
- `cli_optimization_checklists.md`

Active backlog mapping today:

- Card 1: active P0
- Card 4: active P1 cleanup foundation
- Card 5: active P2 batch readability
- Card 6: active P1/P2 recovery-guidance truthfulness
- Card 7: active P3 discoverability polish

Historical / contextual cards:

- Card 2: landed `--replace` healthy-session slice
- Card 3: secondary doc-truthfulness cleanup, not a top active optimization stream

## Card 1

### Title

CLI: make busy and displaced session requests converge on one structured state story

### Priority

P0

### User pain

When a long navigation is in flight or gets interrupted:

- `browser status` / `browser list` / `doctor` say the session is `daemon_unresponsive`
- ordinary commands can hang, emit lower-level transport noise, or otherwise tell a different state story
- users get inconsistent guidance and the CLI feels broken

### Evidence

- real local slow-server repros
- 202 `rpc_webpage(...)` call sites share the same request path
- current structured state already exists for inventory/status surfaces

### Scope

- existing-session request retry exhaustion
- structured busy/unresponsive shell error shape
- protocol round-trip preservation for:
  - `session`
  - `state="incomplete"`
  - `reasons=["daemon_unresponsive"]`
  - `fix`

### Suggested edit points

- `rust/src/cli/connection.rs`
- `rust/src/cli/protocol.rs`
- tests in `rust/src/cli/oneshot.rs` only as needed

### Acceptance checks

- for an in-flight or displaced busy session, ordinary commands no longer expose only lower-level transport/sidecar noise
- shell payload includes:
  - `error.kind="browser_operation"`
  - `error.session=<session>`
  - `error.state="incomplete"`
  - `error.reasons=["daemon_unresponsive"]`
- `browser status` / `browser list` / `doctor` and ordinary commands now tell the same high-level story

### Non-goals

- daemon concurrency rewrite
- faster `browser stop`
- true async navigation

## Card 2

### Title

Historical landed slice: make `browser start --replace` truthful for healthy named sessions

### Priority

Landed

### User pain

This was the first recovery-contract bug that led into the current Project 2 work.

### Evidence

- the flag existed in args/help
- fix text and docs recommended it
- runtime previously ignored `args.replace`
- the installed CLI now no longer shows that behavior on healthy named sessions

### Scope

Historical first-pass truthful behavior:

- `--replace` means restart the named session runtime
- preserve the named session profile

### Suggested edit points

- `rust/src/cli/oneshot.rs`
- `rust/src/cli/args.rs`
- `rust/src/cli/protocol.rs`
- `rust/src/cli/connection.rs`
- `README.md`
- `skills/openpage-test/references/session-management.md`
- `skills/openpage-test/references/cli-smoke.md`

### Acceptance checks

- active-session `browser start --replace` no longer returns plain `already_running=true`
- it performs a stop + start flow for the named session
- resulting session is healthy after restart
- help/docs/fix text no longer imply fresh-state reset semantics on the healthy-session path

### Non-goals

- fresh-state reset for the same session name
- forced cleanup foundation
- orphan Chrome cleanup

## Card 3

### Title

CLI docs: align session-state guidance with persistent named-session profiles

### Priority

P2

### User pain

Current docs still suggest explicit `--user-data-dir` is the main path for profile reuse, while runtime now gives named sessions persistent profile dirs by default.

### Evidence

- runtime default maps named sessions to `OPENPAGE_HOME/profiles/<session>`
- tests already guard that behavior
- session-management docs still imply profile reuse is mostly an explicit `--user-data-dir` workflow

### Scope

- doc and help truthfulness only

### Suggested edit points

- `skills/openpage-test/references/session-management.md`
- possibly `README.md`
- any nearby help text referencing session persistence assumptions

### Acceptance checks

- docs clearly say named sessions default to persistent profile continuity
- docs no longer imply explicit `--user-data-dir` is the only normal reuse path

### Non-goals

- runtime behavior changes
- replace semantics redesign

## Card 4

### Title

CLI: add a safe recovery foundation for forced stop and browser-child cleanup

### Priority

P1/P2 boundary

### User pain

Forced cleanup can return success while Chrome child processes may still survive.

### Evidence

- real local busy-session repro showed forced stop returning success
- Chrome process could remain alive for the session profile
- current cleanup targets daemon pid sidecar, not browser child pid

### Scope

- persist or otherwise externalize browser child cleanup information
- make forced cleanup complete, not daemon-only

### Suggested edit points

- `rust/src/cli/connection.rs`
- `rust/src/cli/serve.rs`
- maybe sidecar helpers / metadata persistence
- possibly `rust/src/browser.rs` only for exposing stable runtime data

### Acceptance checks

- after forced cleanup, no Chrome process remains for the stopped session profile
- daemon sidecars and browser child are cleaned consistently

### First implementation slice

- persist browser pid for the current daemon-backed session model
- use that persisted pid during forced cleanup
- keep graceful stop behavior unchanged

### First tests

- `connection.rs`
  - forced cleanup kills synthetic daemon pid and synthetic browser pid
  - cleanup removes browser-pid sidecar together with existing sidecars
- installed-binary smoke
  - busy session + forced stop leaves no Chrome process for that profile
  - follow-up `browser start --replace` no longer fails on `SingletonLock`

### Main risks

- stale/reused pid safety
- launch-failure timing around when browser pid becomes durable
- future protocol expansion beyond the current one-browser-per-session session model

### Non-goals

- busy-state error semantics
- async navigation

## Card 5

### Title

CLI: make `batch` output readable for mixed-result runs

### Priority

P2

### User pain

Current NDJSON output is hard to scan because users must infer which line belongs to which command.

### Scope

- transcript readability only

### Suggested edit points

- `rust/src/cli/oneshot.rs`
- `rust/src/cli/args.rs`
- `README.md`

### Acceptance checks

- each output line includes enough context to map result -> command
- `--bail` makes the stopping command obvious

### Non-goals

- changing batch execution semantics
- changing the core daemon protocol

## Latest validation status

### Card 1

- First-pass local validation is complete in this branch:
  - installed-binary `title` and `snapshot` now return structured busy/incomplete errors instead of bare `daemon_transient`
- Remaining work under this card is now clearly follow-up scope:
  - stop latency
  - interruptibility
  - larger daemon scheduling changes if needed

### Card 4

- Latest repro reconfirmed the browser-child cleanup gap:
  - `browser stop --session force-probe` returned `forced=true`
  - `browser list` was already empty
  - Chrome for that profile still had to be killed manually
- newest narrowing after `--replace` landed:
  - busy-session `browser start --replace` now attempts a real restart
  - but Chrome can still keep the profile lock alive and make the replacement launch fail
  - this makes Card 4 the main remaining blocker for busy-session recovery under the `--replace` flow
- newly confirmed observability gap after forced cleanup:
  - once the daemon sidecars are removed, `browser list` and `doctor --quick --fix` no longer surface the remaining orphan-browser problem
  - the next visible user-facing failure becomes a profile-lock `browser_launch` error with browser-path-oriented advice
  - acceptance for this card should therefore include restoring truthful recovery guidance after forced cleanup, not only killing the browser child
- repeatability check:
  - two repeated busy + `--replace` runs reproduced the same profile-lock failure on the preserved session profile
  - this is now a stable local recovery-path failure, not a one-off timing artifact
- current adjacent symptom:
  - that profile-lock relaunch failure still surfaces browser-path-oriented `browser_launch` advice
  - acceptance for this card should include making the recovery advice match profile-lock reality once cleanup behavior is fixed
- control-run scope reduction:
  - ordinary `browser stop` on a healthy session shut down cleanly with no orphan Chrome
  - keep this card focused on forced cleanup rather than broader graceful-stop refactors
- code-level implementation narrowing:
  - graceful shutdown already reaches `WebPage::quit()` / `browser.close()`
  - forced cleanup currently only has daemon pid sidecars to work with
  - browser pid already exists in runtime objects, so the missing piece is persistence/externalization rather than discovery
  - current daemon-backed CLI structure makes a per-session browser-pid sidecar plausible for the first pass because tab/window operations stay within the same browser-backed page object

### Card 1 / Card 4 boundary note

- replace-interruption repro showed:
  - `title` / `snapshot` displaced by `--replace` fail as `inactive`
  - the original in-flight `goto` has been seen leaking low-level `io error: daemon port not found` in at least one repro, but repeated stop-interrupt runs more often converged it to structured `inactive`
- treat this as remaining busy/interruption semantic cleanup, not a separate standalone project above the current top three

### Card 1 next slice

- after the first busy truthfulness pass, the next high-value cleanup is:
  - when a session is displaced during an in-flight request, converge that request on the same structured state story instead of leaking low-level transport or sidecar errors

### Card 1 first tests

- `connection.rs`
  - transient exhaustion + post-error inactive session remaps to structured inactive shell error
- installed-binary smoke
  - `goto` displaced by `--replace` no longer exits as low-level `io error: daemon port not found`

### Card 1 main risks

- mistaking ordinary transient I/O noise for a genuinely displaced session
- baking in `inactive` too hard if the product later wants an explicit interrupted/replaced state

### Card 5 next slice

- keep NDJSON output
- add a lightweight per-line batch envelope with:
  - command index
  - original argv or command text
  - underlying command result/error payload
- make the failing/stopping line in `--bail` runs explicit

### Card 5 first tests

- `oneshot.rs`
  - mixed-result batch run emits correlation metadata on every line
  - `--bail` stopping line is explicit
- installed-binary smoke
  - large snapshot payload followed by failure remains easy to correlate from stdout alone

### Card 5 main risks

- compatibility break for raw-payload batch consumers
- over-wrapping the output and making it harder, not easier, to scan

### Card 5

- Installed-binary mixed-result run confirmed:
  - success/failure lines are emitted as raw native JSON only
  - there is no command index or argv echo per line
- Installed-binary `--bail` run confirmed:
  - execution stops after the failing command
  - stdout does not explicitly identify the stopping command beyond its position in the stream

## Card 6

### Title

CLI: align recovery guidance with real forced-stop behavior

### Priority

P1/P2 boundary

### User pain

After forced cleanup defects, the next visible failure can still point users at browser-path advice instead of the actual remaining recovery action.

### Evidence

- repeated busy + `--replace` repros can fail on profile lock after daemon/session truth is already gone
- the visible failure then surfaces as `browser_launch` guidance oriented around browser paths
- `browser logs` / inventory may already be too blind to explain the real problem

### Scope

- recovery-text truthfulness only after cleanup behavior is real

### Suggested edit points

- `rust/src/cli/protocol.rs`
- `rust/src/cli/connection.rs`
- nearby docs/help that currently reinforce stale recovery advice

### Acceptance checks

- profile-lock recovery failures point at the actual remaining cleanup action
- browser-path-oriented advice is not shown first for forced-cleanup defects
- runtime behavior and fix text tell the same recovery story

### Non-goals

- new cleanup design
- broad doctor capability expansion
- unrelated wording cleanup

## Card 7

### Title

CLI: polish command discoverability and follow-up guidance

### Priority

P3

### User pain

The runtime works, but common first-use flows still require guessing the next step.

### Evidence

- `history go` follow-up is not obvious from output/help
- `frame switch` reset targets are under-documented
- `storage get` argument shape is not easy to infer
- non-navigating click can still invite a confusing `wait-for-navigation` timeout

### Scope

- help and follow-up guidance first
- only narrower runtime tightening if wording alone is insufficient

### Suggested edit points

- `rust/src/cli/args.rs`
- `rust/src/cli/oneshot.rs::with_navigation_followup(...)`
- selective `rust/src/cli/serve.rs` call sites only if needed

### Acceptance checks

- common “what do I do next?” mistakes become rare in local use
- output/help makes the normal next step obvious without guesswork

### Non-goals

- recovery correctness
- deep runtime behavior changes
