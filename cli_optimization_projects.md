# OpenPage CLI Optimization Projects

Date: 2026-06-06

This file is the decision-ready summary from long-running local dogfooding of the installed CLI at:

- `/tmp/openpage-cli-eval/bin/openpage`

It is not a design spec for all future work. It is the current shortlist of optimization projects that showed up as real user pain under local use.

Execution companion:

- `cli_optimization_checklists.md`
- `cli_optimization_issue_cards.md`

## Ranking

### 1. Busy-session control plane and interruption semantics

This is the highest-value project.

#### Why it ranks first

- Real local usage shows that a long `goto` or `browser start <url>` can monopolize the daemon request lane.
- During that window:
  - `browser status` / `browser list` / `doctor` classify the session as `daemon_unresponsive`
  - ordinary commands like `title` / `snapshot` can hang, degrade into low-level transport noise, or tell a different session-state story
  - `browser stop` can take tens of seconds before falling back to forced cleanup
- This affects the main CLI surface, not a niche path:
  - `rust/src/cli/oneshot.rs` has 202 `rpc_webpage(...)` call sites
  - they converge through the shared request path in `connection.rs`

#### What the first ship unit should be

Do not start with a large daemon concurrency rewrite.

First ship:

- structured busy/unresponsive error remap for ordinary commands
- aligned state model across:
  - `browser status`
  - `browser list`
  - `browser logs`
  - `doctor`
  - ordinary session-backed commands

#### First implementation slice

- `rust/src/cli/connection.rs`
  - after retry exhaustion for existing-session requests, probe `session_target_state(...)`
  - if it is `Unresponsive`, stop returning only low-level transient transport noise
- `rust/src/cli/protocol.rs`
  - preserve structured round-trip fields for busy sessions:
    - `session`
    - `state="incomplete"`
    - `reasons=["daemon_unresponsive"]`
    - `fix`

Recommended first public shape:

- `error.kind = "browser_operation"`
- `error.session = <session>`
- `error.state = "incomplete"`
- `error.reasons = ["daemon_unresponsive"]`

This keeps protocol churn smaller than introducing a brand-new public error kind immediately.

#### What it does not solve yet

- daemon accept-loop starvation
- fast interruptibility
- true non-blocking navigation

#### Recommended next slice after the current branch

- keep working in the same central hook, not per-command patches
- next ship unit:
  - when an existing-session request fails transiently but the session has already become inactive/displaced, converge on the structured inactive story instead of leaking low-level transport/sidecar errors or divergent states
- why this is the right next slice:
  - current installed-binary evidence already shows `title` / `snapshot` displaced by `--replace` fail as `inactive`
  - repeated stop-interrupt runs now more often converge the original in-flight `goto` to `inactive`
  - but earlier local repros still proved that this same shared path can leak lower-level `io error: daemon port not found`
  - the real project is therefore broader than one old symptom string: it is about making interrupted commands tell one coherent session-state story

Those are follow-up phases, not the first slice.

## 2. Forced-stop cleanup and recovery truthfulness

This is the second-highest-value project.

#### Why it ranks second

The CLI still recommends recovery actions that are not equally real in the hardest busy-session cases.

Important current truth:

- healthy-session `--replace` is now real
- but busy-session recovery still breaks on forced-stop cleanup:
  - forced cleanup can remove daemon/session truth while the browser child still survives
  - a follow-up `--replace` can then fail on Chrome profile lock
  - the next visible user-facing failure can still surface as browser-launch advice that points at browser paths rather than the real cleanup defect

#### What is already landed

The minimal truthful meaning of `--replace` is now already proven on the installed CLI:

- restart this named session runtime
- preserve the session profile

Real local dogfooding confirmed:

- localStorage survived stop/start on the same named session
- `browser start --session <name> --replace ...` no longer falls through to `already_running=true` on healthy sessions

#### What the first ship unit should be

The next ship unit is no longer initial `--replace` implementation.

The real remaining problem is:

1. forced cleanup must terminate the browser child, not only the daemon pid
2. recovery guidance must stay truthful until that cleanup is complete

#### First implementation slice

- `rust/src/cli/connection.rs`
  - extend forced cleanup beyond daemon pid sidecars
  - preserve enough observability that recovery guidance does not go blind too early
- `rust/src/cli/serve.rs`
  - persist a browser-child cleanup handle for daemon-backed sessions
- only after behavior is real, update wording in:
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/connection.rs`
  - nearby docs/help that currently amplify browser-path-oriented recovery advice

#### What it does not solve yet

- a true fresh-state reset for the same session name
- broad `doctor --quick --fix` expansion into busy-session recovery
- full confidence under every daemon-busy edge case before cleanup behavior itself is fixed

## 3. Batch readability and composition UX

This is a real issue, but clearly below the first two.

#### Why it ranks third

- batch behavior worked in local usage
- the main problem is readability:
  - output is raw NDJSON
  - humans have to infer which line belongs to which command

#### What the first ship unit should be

- add command index and/or echoed argv per output line
- make `--bail` output clearly identify which command stopped execution

#### First implementation slice

- keep the current NDJSON transport style
- wrap each emitted line in a minimal batch envelope that includes:
  - command index
  - original argv/command text
  - underlying command result/error payload
- make the stopping line in `--bail` mode explicit instead of relying on stream position alone

#### Why it is not first

- it is transcript UX friction, not runtime correctness or recovery truthfulness
- it does not block core CLI operation the way busy-session behavior does

## 4. Command discoverability and follow-up guidance polish

This is lower priority than the first three, but it showed up repeatedly in local use.

#### Why it ranks fourth

- core flows like tab/frame/history/storage worked
- the friction is mostly first-use ergonomics:
  - `history go` wants the usual navigation follow-up, but output/help does not make that obvious
  - `frame switch` supports reset targets like `main` / `root` / `page`, but help does not surface them well
  - `storage get` argument shape is not very discoverable on first use
  - non-navigating click can still emit a `navigation_token`, which invites an unnecessary wait-follow-up timeout

#### What the first ship unit should be

- tighten help text first
- then tighten follow-up guidance where output is still misleading in normal use

#### First implementation slice

- `rust/src/cli/oneshot.rs`
  - `with_navigation_followup(...)`
- `rust/src/cli/args.rs`
  - click/history/frame/storage help text
- only use narrower runtime changes if help/output wording proves insufficient

#### Why it is not higher

- the flows are usable today
- the pain is real but mostly ergonomic
- it should not displace correctness and recovery work

## Not top-level projects right now

### `doctor --quick --fix` broadening

This is not currently a broken contract.

Docs already say `doctor --quick --fix` is intentionally narrow:

- legacy residue cleanup
- stale sidecars
- incompatible daemon sessions
- incomplete unready daemon sessions

If it later grows to handle busy sessions, that should be treated as a deliberate capability expansion, not a bug fix.

### Fresh-state reset semantics for `--replace`

This may be worth doing in the future, but it is not the right first move.

Reason:

- current named-session design already implies persistent profile continuity
- a fresh-state reset would be a materially heavier semantic and implementation change

### Large daemon scheduling rewrite

This may still be necessary eventually, but it is not the first recommended optimization stream.

Reason:

- a smaller truthfulness-focused slice can deliver high user value earlier
- async navigation and/or busy-state ownership may reduce urgency before a large rewrite

## Recommended sequence

1. Busy interruption-semantics slice
2. Forced-stop browser-child cleanup foundation
3. Recovery guidance truthfulness
4. Batch readability
5. Discoverability and follow-up guidance polish
6. Async/non-blocking navigation behavior only if earlier slices are insufficient
7. Large daemon scheduling refactor only if earlier slices are insufficient

## Latest branch status

- Busy truthfulness Slice A is now validated on the installed binary:
  - during an in-flight slow navigation, `title` and `snapshot` return:
    - `error.kind="browser_operation"`
    - `error.state="incomplete"`
    - `error.reasons=["daemon_unresponsive"]`
  - they no longer collapse to bare `daemon_transient`
- Recovery-contract truthfulness remains the next priority:
  - `browser stop` can return `forced=true`
  - `browser list` can already be empty
  - Chrome for that session profile may still survive until manually killed
- `--replace` has now moved from "fake public contract" to "partially landed recovery primitive":
  - for a healthy named session it now performs a real restart and preserves the same profile continuity
  - for a busy session it now attempts the restart, but recovery can still fail on Chrome profile locks while the orphan browser process survives
- startup-observation recheck did not dislodge the current ranking:
  - immediate `browser list` / `browser status` checks after fresh starts came back healthy in the latest retest
- deeper batch dogfood did reinforce Project 3:
  - mixed success/failure runs still emit only raw per-command JSON lines
  - `--bail` still provides no explicit indication of which command stopped execution
- code inspection also narrowed the smallest edit surface:
  - `rust/src/cli/oneshot.rs::run_batch(...)` currently just prints each command's native payload
  - the first fix can stay local to output shaping rather than touching execution order or daemon protocol

## Updated recovery-project split

### Done enough for now

- `browser start --session <name> --replace ...`
  - now genuinely restarts a healthy named session
  - no longer falls through to `already_running=true`

### Still the real remaining problem

- forced-stop / orphan-Chrome cleanup
  - this is now the blocking piece for busy-session recovery
  - local installed-binary repro shows that replace can stop the old daemon truth, yet still fail to launch the replacement browser because the old Chrome process kept the profile lock
  - control-run evidence now shows normal `browser stop` shuts down cleanly
  - so this is specifically a forced cleanup defect, not a generic shutdown problem
  - code evidence now narrows the missing mechanism:
    - graceful stop reaches `WebPage::quit()` -> `browser.close()`
    - forced cleanup only kills the daemon pid from sidecars
    - browser child pid already exists in runtime objects, but is not persisted into the daemon-session cleanup path
    - the current daemon-backed CLI model also makes a first-pass per-session browser-pid sidecar plausible:
      - tab/window operations stay inside the same browser-backed `ServeWebPage.page`
      - target switches reuse that browser object rather than launching an extra browser process
- recovery-path error specificity
  - profile-lock relaunch failures currently surface as generic `browser_launch` with browser-path advice
  - that is downstream of the same recovery project, not a separate top-level stream
- interruption semantics after replace
  - once `--replace` displaces an in-flight busy session, some commands now fail as `inactive`
  - but the original in-flight `goto` can still leak out as `daemon_transient`
  - this remains part of the busy/error-semantic project rather than a new standalone ranking item
