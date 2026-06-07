# Notes: Unified config.toml refactor (2026-06-01)

## CLI dogfooding roadmap notes (2026-06-06)

### Objective
- Use the locally installed `openpage` binary for real workflows.
- Find optimization projects that matter over time, not just single-command paper cuts.

### Installed binary
- Path: `/tmp/openpage-cli-eval/bin/openpage`
- Reinstall command:
  - `cargo install --path rust --bin openpage --root /tmp/openpage-cli-eval --force`

### Confirmed high-friction areas so far
- Startup / diagnostics / recovery consistency remains the top shell-level issue family.
- `doctor --quick` previously looked too optimistic when a configured browser path merely resolved but had not been launch-validated.
- `batch` output is functional but still light on execution context for humans reading failure streams.

### Latest fix landed this session
- `doctor --quick` no longer marks `browser.executable` as `pass` when it only verified path resolution.
- Instead it reports `info` and points users to rerun full `openpage doctor` for a real launch smoke test.
- `browser status` no longer reports a session as healthy when the daemon-side page receiver is already gone; it now marks the session `incomplete` with `reasons=["broken_target"]` and an explicit recreate/recover fix.

### Next evidence to collect
- Clean first-run experience with a fresh `OPENPAGE_HOME`
- Multi-step happy path (`browser start` -> `wait-for-ready` -> `snapshot` / `title` -> `browser stop`)
- `batch` failure readability and composition ergonomics

### New runtime findings from installed-binary dogfooding
- Fresh-home first-run flow is now reasonably legible:
  - `--help`
  - `doctor --quick`
  - fail-fast `title --session ...`
  - `browser start --session ... --headless https://example.com`
  - `wait-for-ready`
  - `title`
  - `snapshot`
  - `browser stop`
- `batch` is usable, but still forces humans to infer which line belongs to which command because output is pure NDJSON with no command index/echo.
- More importantly, the current CLI launch model still leaves a structural risk around browser debug-port policy:
  - active CLI config can still show `auto_port=false`
  - a second session launch while another default-config session is active can create a broken runtime
  - this is not just a bad error message; it is a launch-model mismatch for a multi-session CLI
- Concrete reproduced bad state:
  - one session (`reuse-b`) was left active
  - another session (`loop-1`) launched under the same default debug-port semantics
  - `title --session loop-1` then failed with `page operation failed: send failed because receiver is gone`
  - before this pass, `browser status --session loop-1` still said `state=healthy`
  - after this pass, `browser status --session loop-1` says `state=incomplete`, `reasons=["broken_target"]`
- Broader consistency gap still open:
  - `browser list` still reports the same broken session as healthy
  - `doctor --quick` still reports the same broken session as healthy
  - so the long-term project is larger than the `browser status` patch

### Current ranking of optimization projects
1. Failure semantics / runtime health truthfulness across `browser status`, `browser list`, and `doctor`
2. Startup robustness and browser debug-port allocation policy for a multi-session CLI
3. Batch composition readability for humans consuming NDJSON failure streams

### This session's deeper evidence (2026-06-06, later pass)
- I broadened the inventory path so that a session proven to have:
  - `missing_target`
  - `broken_target`
  is now classified as `incomplete` instead of being treated as a healthy daemon session.
- Verified at code/test level:
  - `browser status` fix from the prior pass remains valid
  - new targeted tests now pin:
    - runtime issue -> incomplete reasons
    - inventory payload -> incomplete state + recovery fix for `broken_target`
- Real installed-binary retest exposed a second failure class that is not the same as `broken_target`:
  - normal `keeper` session started fine
  - second `broken-inventory` session got into a bad state where:
    - `browser start` eventually surfaced `daemon_transient` / `os error 35`
    - `browser list` still showed the session as healthy
    - `doctor --quick` still showed the session as healthy
    - direct probes like `title` / `browser status` could hang for a long time
- Interpretation:
  - the earlier `broken_target` patch was correct but not sufficient
  - there is another runtime-health bucket where the daemon is TCP-ready yet not responsive enough for command truthfulness
  - this strengthens the case that the real long-term project is not one bugfix, but a broader runtime health model

### Revised project framing
1. **Runtime health truthfulness**
   - unify what counts as healthy across:
     - `browser status`
     - `browser list`
     - `doctor`
   - include not only `missing_target` / `broken_target`, but also daemon-transient / partially-responsive runtime states
2. **Startup and port policy**
   - multi-session startup with default fixed debug-port semantics still produces unstable states
   - this is likely upstream of part of the runtime-health confusion
3. **Batch human readability**
   - still real, but clearly below the two system-level issues above

### New verified progress on runtime health truthfulness
- Added a short-timeout runtime probe for health classification, separate from the normal command timeout budget.
- New runtime issue bucket:
  - `daemon_unresponsive`
- Verified with focused tests:
  - `incomplete_daemon_fix_for_unresponsive_session_points_to_logs_and_restart`
  - `daemon_status_payload_json_marks_unresponsive_target_as_incomplete`
  - `daemon_inventory_payload_marks_runtime_unresponsive_as_incomplete`
- Verified with the installed binary in a real multi-session scenario:
  1. start `keeper`
  2. start second session `probe-bad`
  3. second start falls into the problematic transient state
  4. now:
     - `browser status --session probe-bad` returns `state="incomplete"` and `reasons=["daemon_unresponsive"]`
     - `browser list` puts `probe-bad` under `incomplete[]`, not `sessions[]`
     - `doctor --quick` reports `daemon.incomplete.probe-bad` with the same reason
- This is the strongest evidence so far that the top project is correct:
  - the CLI needed a richer runtime health model more than it needed more commands

### New paired launch-policy experiment (2026-06-06, latest pass)
- Current code facts:
  - default runtime launch still seeds `address = 127.0.0.1:9222`
  - default config path leaves `auto_port=false` unless the config chain overrides it
- Real installed-binary paired experiment:

#### A. Default policy
- Environment:
  - fresh `OPENPAGE_HOME`
  - `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Steps:
  1. `browser start --session default-a --headless https://example.com`
  2. `browser start --session default-b --headless https://example.com`
- Result:
  - `default-a` healthy
  - `default-b` quickly classified as `daemon_unresponsive`
  - `browser list` showed:
    - `healthy=1`
    - `incomplete=1`
  - `browser status --session default-b` returned `state="incomplete"`, `reasons=["daemon_unresponsive"]`
  - `browser stop --session default-b` needed `forced=true`

#### B. auto_port policy
- Config used:
  - `/tmp/openpage-auto-port-config.toml`
  - `[browser]`
  - `executable_path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
  - `auto_port = true`
- Environment:
  - fresh `OPENPAGE_HOME`
  - `OPENPAGE_CONFIG=/tmp/openpage-auto-port-config.toml`
- Steps:
  1. `browser start --session auto-a --headless https://example.com`
  2. `browser start --session auto-b --headless https://example.com`
  3. `title --session auto-a`
  4. `title --session auto-b`
  5. `browser list`
  6. `doctor --quick`
- Result:
  - both starts succeeded
  - both `title` commands returned `Example Domain`
  - `browser list` showed:
    - `healthy=2`
    - `incomplete=0`
  - `doctor --quick` showed both daemon sessions as healthy
  - both `browser stop` calls completed without force

### Stronger prioritization from this evidence
1. **Startup / port-allocation policy is now very likely the highest-leverage upstream project**
   - because a one-line policy change (`auto_port=true`) materially changed the real multi-session outcome
   - this is not just nicer ergonomics; it removes a runtime corruption path
2. **Runtime health truthfulness remains critical**
   - because without it, the default-policy failure would still look healthy
   - but it is now clearly downstream protection around a launch-policy flaw
3. **Batch readability stays third**
   - useful, but much less structural than the first two

### Follow-up experiment: `--port 0` looks like the safer implementation direction
- Why test this:
  - `auto_port=true` fixed the multi-session instability
  - but it also switched browser profiles to temp directories and destroyed stop/start persistence
- Real installed-binary test with `--port 0` under otherwise default policy:

#### Multi-session health
- fresh `OPENPAGE_HOME`
- commands:
  - `browser start --session zero-a --port 0 --headless https://example.com`
  - `browser start --session zero-b --port 0 --headless https://example.com`
  - `title --session zero-a`
  - `title --session zero-b`
  - `browser list`
- result:
  - both starts succeeded
  - both `title` commands succeeded
  - `browser list` showed both sessions as healthy
  - on-disk profiles existed at:
    - `profiles/zero-a`
    - `profiles/zero-b`

#### Same-session persistence
- fresh `OPENPAGE_HOME`
- commands:
  1. `browser start --session zero-persist --port 0 --headless https://example.com`
  2. `js "localStorage.setItem(...); localStorage.getItem(...)" --session zero-persist`
  3. `browser stop --session zero-persist`
  4. `browser start --session zero-persist --port 0 --headless https://example.com`
  5. `js "localStorage.getItem(...)" --session zero-persist`
- result:
  - stored value survived stop/start
  - profile directory remained at `profiles/zero-persist`

### Refined project statement
- The highest-value upstream optimization project is no longer best phrased as:
  - "default auto_port=true"
- The better statement is:
  - **make daemon-backed session launches use dynamically assigned debugging ports by default while preserving session-scoped persistent user-data directories**
- In this codebase, that likely means:
  - decouple "dynamic debug port" from the current `auto_port=true` semantics
  - or internally prefer the effective equivalent of `--port 0` for daemon-backed session launches when the debugger port/address came from built-in defaults
- Important implementation constraint:
  - current config resolution tracks source for browser path and user-data-dir, but not for debugger address/port
  - so a safe implementation likely needs source-aware handling for debugger port/address before changing built-in behavior

### New recovery-path evidence from continued installed-binary dogfooding (2026-06-06, later pass)
- I extended the local runs beyond startup and status into actual recovery workflows.
- Installed binary remained `/tmp/openpage-cli-eval/bin/openpage`.

#### A. Normal asynchronous navigation contract is mostly sound
- In a fresh `OPENPAGE_HOME`, the following workflow behaved as designed:
  1. `browser start --session recoverdog --headless about:blank`
  2. `goto --session recoverdog http://127.0.0.1:8878`
  3. `wait-for-navigation --session recoverdog --token nav-2`
  4. `wait-for-ready --session recoverdog`
  5. `title --session recoverdog`
- Result:
  - `goto` returned a `navigation_token`
  - `wait-for-navigation` and `wait-for-ready` both eventually succeeded
  - `title` eventually returned `slow2`
- Interpretation:
  - the navigation token / wait follow-up contract itself is valid
  - the real UX break remains the in-flight busy window, not the eventual completion path

#### B. `doctor --quick --fix` does not recover busy sessions
- During an in-flight slow navigation, I ran:
  - `doctor --quick --fix`
- Result:
  - the session was reported as `daemon.incomplete.recoverdog`
  - `reasons=["daemon_unresponsive"]`
  - `fixed=[]`
  - the session remained incomplete
- Interpretation:
  - current `doctor --fix` only cleans incomplete unready sessions or incompatible versions
  - it does not act on the busy/unresponsive class that real users are now likely to hit

#### C. Recovery advice points users to logs that may be empty
- During the same busy session, I ran:
  - `browser logs --session recoverdog --tail 20`
- Result:
  - the command returned `log_exists=true`
  - but `content=""`
  - and `log_hint="Log file exists but is empty..."`
- Interpretation:
  - current fix text frequently says "inspect the daemon log"
  - but in the busy/unresponsive case the log can provide no actionable information at all
  - this is a real recovery UX mismatch, not just a missing sentence

#### D. `browser start --replace` is currently promised but not implemented
- This is the strongest new finding from this pass.
- Code audit:
  - `rust/src/cli/args.rs` exposes `BrowserStartArgs.replace`
  - `openpage help browser start` documents `--replace` as "Replace existing session if it exists"
  - `rust/src/cli/oneshot.rs::start_browser(...)` never reads `args.replace`
  - `rust/src/cli/serve.rs::create_webpage(...)` has no replace behavior; if a target already exists it simply returns `{"existing": true}`
- Command evidence:
  1. `browser start --session repdog --headless about:blank`
  2. `goto --session repdog http://127.0.0.1:8878`
  3. while the session was incomplete/busy, run:
     - `browser start --session repdog --replace --headless https://example.com`
- Result:
  - the command took about 21 seconds
  - it returned `{"already_running": true, ...}`
  - same daemon pid / port remained in place
  - session eventually became healthy and navigated, but no actual replace path was exercised
- Interpretation:
  - `--replace` is currently a contract bug, not merely a suboptimal implementation
  - the CLI advertises a recovery primitive that does not exist end-to-end

### Revised ranking after the recovery pass
1. **Busy-session control plane / activity semantics**
   - still the biggest optimization project
   - because in-flight navigation can still monopolize command handling and degrade ordinary commands into `daemon_transient`
2. **Recovery contract truthfulness**
   - now clearly a project of its own, not just part of diagnostics polish
   - includes:
     - `doctor --quick --fix` no-op on busy sessions
     - empty-log recovery dead ends
     - `browser start --replace` being documented but non-functional
     - forced stop orphaning Chrome children
3. **Batch output readability**
   - still real, but now further behind the system-level recovery issues above

### Concrete project framing from this pass
1. **Busy-session state + command error contract**
   - establish an explicit busy/unresponsive state instead of surfacing generic `daemon_transient`
   - make ordinary `rpc_webpage(...)` callers converge on the same user-facing recovery story
2. **Real recovery primitives**
   - either implement actual replace semantics end-to-end, or remove the flag/help/fixes until it exists
   - teach `doctor --fix` what it may safely do for busy sessions
   - make forced cleanup capable of terminating the browser child, not only the daemon sidecar pid
3. **Evidence-quality diagnostics**
   - avoid recommending logs as a first-line fix when the busy path usually yields empty logs
   - prefer recovery advice grounded in what the CLI can actually do automatically

### New scope quantification pass (2026-06-06, latest pass)
- I used the current worktree to estimate how broad the highest-friction issues really are.

#### A. Busy-session error semantics affect almost the whole session command surface
- `rust/src/cli/oneshot.rs` currently contains **202** `rpc_webpage(...)` call sites.
- `rpc_webpage(...)` is only a tiny wrapper around `rpc_request_existing(...)`:
  - `rpc_webpage(...)` -> `rpc_request_existing(...)` -> `send_request_existing(...)`
- Meaning:
  - most session-backed commands share the same transport/error entry point
  - the current busy-session pain is not command-specific UX debt
  - it is a central contract problem with unusually high leverage

#### B. There is already a central structured-error shell for a project-level fix
- `rust/src/cli/protocol.rs` already centralizes:
  - stable `error.kind`
  - `fix`
  - `session`
  - `state`
  - `reasons`
  - `retryable`
  - `suggested_action`
- Current busy/unresponsive transport failures still collapse into:
  - kind: `daemon_transient`
  - fix: `Retry the same command.`
- Interpretation:
  - the CLI already has enough structured error machinery to carry a richer busy-state contract
  - the missing piece is not serialization shape, but the classification and mapping policy

#### C. `--replace` contract pollution is now larger than the flag itself
- The problem is no longer just:
  - args define `replace`
  - implementation ignores it
- The current tree actively spreads that promise through multiple layers:
  - `openpage help browser start`
  - `rust/src/cli/connection.rs` fix text for:
    - `missing_target`
    - `broken_target`
    - `daemon_unresponsive`
  - `rust/src/cli/protocol.rs` fix text for startup/browser-path recovery
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - this is a recovery-contract project, not a local parameter plumbing bug
  - even if implementation is fixed, docs/tests/fix text need a coordinated sweep

### Stronger project boundary after the quantification pass
1. **Busy-state / control-plane project**
   - main value:
     - one central change can improve roughly the whole session command surface
   - likely loci:
     - `rust/src/cli/connection.rs`
     - `rust/src/cli/protocol.rs`
     - maybe `rust/src/cli/serve.rs`
2. **Recovery-contract project**
   - main value:
     - stop the CLI from recommending recovery moves that are not real
   - includes:
     - `--replace`
     - `doctor --fix` busy-session policy
     - forced stop browser-child cleanup
     - empty-log recovery dead ends
3. **Batch output project**
   - still clearly separate and lower risk
   - mostly isolated to `run_batch(...)` output shaping

### New implementation-feasibility narrowing (2026-06-06, latest pass)

#### A. Busy/error remapping likely has a real central hook
- Current shared chain:
  - `rpc_webpage(...)`
  - `rpc_request_existing(...)`
  - `send_request_existing(...)`
  - `send_request_with_retry(...)`
- `send_request_existing(...)` already sits immediately beside:
  - `session_target_state(...)`
  - short-timeout probe logic that can classify:
    - `Missing`
    - `Broken`
    - `Unresponsive`
- Interpretation:
  - the busy/unresponsive project likely does **not** need per-command edits
  - the most credible implementation direction is:
    - let request retry exhaust as it does now
    - then probe session runtime state
    - remap the final generic transient into a richer busy/unresponsive session error before it reaches the 202 `rpc_webpage(...)` callers

#### B. Forced cleanup does not need browser pid discovery from scratch
- Current code facts:
  - `BrowserState` already stores `browser_pid`
  - `Browser::browser_pid()` exposes it
  - `WebPage::browser_pid()` exposes it too
- But current daemon sidecars only persist:
  - daemon pid
  - daemon port
  - version
- Interpretation:
  - the force-stop project is probably not blocked on "can we know the browser child pid?"
  - the real gap is:
    - exporting that runtime truth to a cleanup layer that survives daemon unresponsiveness
  - plausible directions:
    - persist browser pid in a sidecar alongside daemon pid
    - or add a dedicated persisted runtime metadata file written when the page/browser is created

#### C. `--replace` is likely medium scope, not trivial plumbing
- Why:
  - `start_browser(...)` ignores `args.replace`
  - `create_webpage(...)` returns `existing=true` before any launch/recreate logic
  - multiple fix texts and docs already depend on replace semantics
- Interpretation:
  - implementing replace means choosing one real behavior contract:
    - stop daemon session, wait for teardown, recreate browser/page target, then optionally navigate
    - or a lighter in-daemon target recreation flow
  - either way, this is larger than "pass one bool into the request"

### Important nuance from the public-contract audit
- `doctor --quick --fix` and `--replace` are not the same class of problem.

#### A. `doctor --quick --fix` is currently narrow by design, and the docs say so
- Public help and docs explicitly say:
  - it fixes legacy session JSON residue
  - stale sidecars discovered during inventory
  - incompatible daemon sessions
  - incomplete daemon sessions only when they are unready
- Interpretation:
  - busy-session recovery is currently a **product gap**, not a broken `doctor --fix` contract
  - if expanded, that should be a deliberate new capability with safety rules, not a stealth behavior tweak

#### B. `browser start --replace` is a broken public contract today
- Public help says:
  - "Replace existing session if it exists"
- Skill/reference docs also teach it as the clean-restart path for a known session name.
- Runtime implementation does not honor that promise.
- Interpretation:
  - `--replace` is a true contract bug
  - it is the sharpest example inside the broader recovery-contract project

### Project briefs

#### Project 1: Busy-session control plane and error semantics
- User problem:
  - while a long navigation is in flight, control commands and ordinary page commands degrade into mixed signals:
    - `browser status` / `browser list` / `doctor` say `daemon_unresponsive`
    - ordinary commands still end as generic `daemon_transient`
    - stop can take tens of seconds and may force-kill only the daemon
- Why this is a project:
  - the issue is structural in the daemon execution model and shared error path
  - a central fix can improve roughly the entire session command surface
- Likely files:
  - `rust/src/cli/serve.rs`
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/doctor.rs`
- Verification criteria:
  - reproduce with a real slow local HTTP server
  - during in-flight navigation:
    - `status` / `list` / `doctor` / ordinary commands emit one coherent busy-state story
    - ordinary commands no longer collapse to a bare "Retry the same command."
    - `stop` / `logs` remain predictably usable

##### Practical phase split
- Phase 1: **semantic correction**
  - likely centered in `connection.rs` + `protocol.rs`
  - goal:
    - after request retry exhaustion, probe runtime state and remap generic transient failures into a richer busy/unresponsive session error
  - value:
    - improves most user-facing commands quickly
  - limitation:
    - does **not** remove the daemon serial-control bottleneck
    - `stop` can still block for a long time if the daemon cannot accept control requests promptly
- Phase 2: **control-plane availability**
  - likely requires `serve.rs` work
  - goal:
    - stop long page operations from monopolizing the daemon’s only request lane
  - value:
    - addresses the actual responsiveness root cause
  - implication:
    - this is the part that turns the project from "better truthfulness" into "actually less annoying to use"

#### Project 2: Recovery-contract truthfulness
- User problem:
  - the CLI recommends recovery actions that are not equally real or equally useful
- Confirmed sub-issues:
  - `browser start --replace` is documented but non-functional
  - forced stop can orphan Chrome children
  - busy recovery advice can send users to empty logs
  - `doctor --fix` intentionally does not repair busy sessions today
- Why this is a project:
  - this spans help text, fix text, runtime behavior, docs, and tests
  - solving only one layer leaves the contract inconsistent
- Likely files:
  - `rust/src/cli/args.rs`
  - `rust/src/cli/oneshot.rs`
  - `rust/src/cli/serve.rs`
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/protocol.rs`
  - `skills/openpage-test/references/*`
  - `README.md`
- Verification criteria:
  - every suggested recovery action is either:
    - implemented and works in a real local session
    - or removed from help/fix/docs
  - forced stop leaves no orphan Chrome for the stopped session profile
  - real recovery runs match the shell guidance users are shown

##### Practical phase split
- Phase 1: **contract cleanup**
  - shortest path options:
    - implement `--replace` at the oneshot layer as an explicit stop-then-start flow
    - or remove `--replace` from public guidance until deeper cleanup exists
  - likely files:
    - `rust/src/cli/args.rs`
    - `rust/src/cli/oneshot.rs`
    - `README.md`
    - `skills/openpage-test/references/*`
- Phase 2: **real cleanup foundation**
  - needed if `--replace` or forced stop is meant to be trustworthy under daemon-busy failure modes
  - likely direction:
    - persist browser child pid or equivalent runtime metadata outside daemon memory
    - extend forced cleanup beyond daemon pid only
  - likely files:
    - `rust/src/cli/connection.rs`
    - `rust/src/cli/serve.rs`
    - maybe a new sidecar path/helper set
- Important dependency:
  - a oneshot-level `--replace` implementation is plausible without protocol changes
  - but it still inherits today’s forced-stop orphan-Chrome risk unless the cleanup foundation is fixed too

### New semantic result for `--replace`
- I tested the simplest plausible implementation strategy:
  - stop the existing named session
  - start the same named session again
- Real local result:
  - session: `semdog`
  - set `localStorage['replace_probe']='v1'`
  - stop the session
  - restart the same named session
  - read `localStorage['replace_probe']`
  - value was still `v1`
- Code context matches the runtime result:
  - built-in defaults map named sessions to persistent profile dirs under `OPENPAGE_HOME/profiles/<session>`
  - stop/start of the same session name naturally reuses that profile
- Interpretation:
  - a minimal oneshot stop-then-start implementation of `--replace` would be:
    - a process/page restart
    - **not** a fresh browser-state reset
  - so the public meaning of `--replace` still needs an explicit decision

### `--replace` semantic options now look like this
1. **Restart same session process, preserve profile**
   - shortest implementation
   - aligns with current named-session persistence model
   - does not clear cookies/localStorage
2. **Restart with fresh profile for the same session name**
   - closer to many users' "clean restart" intuition
   - conflicts with the current model where session name implies profile continuity
   - likely needs explicit profile cleanup semantics
3. **Recreate only the page target inside the same browser/profile**
   - matches some current fix text around broken/missing target
   - does not guarantee a true browser-process restart

### New protocol constraint for Busy Phase 1
- A central busy-state remap still looks viable, but current round-trip helpers impose one extra requirement.
- Current `protocol.rs` structured-error reconstruction only reliably preserves:
  - `inactive`
  - `incompatible` / `version_mismatch`
  - `incomplete` + `daemon_not_ready`
  - `daemon_transient` retry metadata
- It does **not** currently reconstruct `state="incomplete"` with `reasons=["daemon_unresponsive"]` from a generic `BrowserOperation` message.
- Interpretation:
  - Busy Phase 1 likely needs coordinated work in:
    - `connection.rs` to classify/remap after retry exhaustion
    - `protocol.rs` to preserve the busy/unresponsive state across local and daemon error round-trips
- Most plausible minimal strategy:
  - in `send_request_existing(...)`, when request retries exhaust into a transient failure:
    - probe `session_target_state(...)`
    - if it is `Unresponsive`, return a structured browser/session error instead of generic `daemon_transient`
  - extend protocol reconstruction so that this new canonical busy message round-trips back to:
    - `error.kind`
    - `error.session`
    - `error.state="incomplete"`
    - `error.reasons=["daemon_unresponsive"]`
    - appropriate `error.fix`

### Recommendation matrix

#### Busy Phase 1: candidate approaches

##### Option A: keep public `error.kind` conservative, enrich structured state
- Shape:
  - keep `error.kind="browser_operation"` for busy-session ordinary commands
  - add/round-trip:
    - `error.session`
    - `error.state="incomplete"`
    - `error.reasons=["daemon_unresponsive"]`
    - a fix that points to `status/logs/stop/restart`
- Pros:
  - smallest protocol churn
  - fits the current structured-error model
  - avoids introducing a brand-new public error kind immediately
- Cons:
  - less explicit than a dedicated busy-specific kind
  - callers must inspect `state/reasons`, not only `kind`
- Recommendation:
  - **best first move**
  - it gives users and shell callers a much truer story without widening the public error taxonomy yet

##### Option B: introduce a new public busy-specific kind
- Shape:
  - e.g. `error.kind="session_unresponsive"` or similar
  - still carry `session/state/reasons/fix`
- Pros:
  - semantically clearer for machine callers
  - easier to branch on without inspecting `state/reasons`
- Cons:
  - broader churn:
    - protocol tests
    - docs
    - compatibility expectations
  - easier to get wrong before the semantics settle
- Recommendation:
  - defer until after Option A proves the taxonomy and recovery guidance are right

#### Replace Phase 1: candidate approaches

##### Option A: implement `--replace` as stop + start same named session, preserving profile
- Shape:
  - if session exists:
    - `browser stop --session <name>`
    - `browser start --session <name> ...`
  - continue using the same `OPENPAGE_HOME/profiles/<session>` mapping
- Pros:
  - shortest path to make the flag truthful
  - matches the current runtime/session design and persistence tests
  - no daemon protocol change required
- Cons:
  - does not clear cookies/localStorage
  - current doc wording like "clean restart" becomes misleading unless rewritten
  - still inherits forced-stop cleanup weaknesses
- Recommendation:
  - **best Phase 1 implementation path**
  - but only if docs/help/fix text are rewritten to mean "restart this named session runtime", not "fresh state"

##### Option B: define `--replace` as fresh-state reset for the same session name
- Shape:
  - stop session
  - remove or rotate the session profile dir
  - start session again
- Pros:
  - matches many users' intuitive reading of "replace" / "clean restart"
- Cons:
  - conflicts with the current named-session persistence model
  - higher data-loss risk
  - larger implementation and documentation burden
- Recommendation:
  - too heavy as the first truthful fix unless product intent explicitly prefers stateless replace

##### Option C: remove public `--replace` guidance until a stronger implementation exists
- Shape:
  - remove flag/help/fix/doc references for now
- Pros:
  - fastest way to stop lying to users
  - avoids prematurely locking in semantics
- Cons:
  - gives up a useful recovery shorthand
  - pushes more recovery burden onto manual `stop` + `start`
- Recommendation:
  - viable fallback if runtime implementation is deferred, but weaker than Option A

### New doc/runtime inconsistency worth tracking
- Current runtime and tests intentionally give named sessions persistent profile dirs by default.
- But `skills/openpage-test/references/session-management.md` still says:
  - start with explicit `--user-data-dir` only when you explicitly want profile reuse
- That statement is no longer the full truth for current daemon-backed named sessions.
- Interpretation:
  - this is not a new top-level optimization project
  - but it should be cleaned up as part of:
    - replace semantics clarification
    - overall session-state discoverability/documentation

### First test additions that would de-risk implementation

#### Busy Phase 1
- Existing coverage is strong on:
  - inventory/status classification for `SessionTargetState::Unresponsive`
  - generic `daemon_transient` shaping
- Missing coverage is the actual bridge between those two worlds:
  - when an ordinary session command hits retry exhaustion
  - and a short runtime probe says `Unresponsive`
  - the top-level shell payload should preserve the richer busy session state instead of generic retry-only transient text
- Best first tests:
  1. `connection.rs`
     - a targeted unit test around the new remap path after retry exhaustion
     - verify the remap uses `session_target_state == Unresponsive`
  2. `protocol.rs`
     - a round-trip reconstruction test for:
       - `kind="browser_operation"`
       - `session=<name>`
       - `state="incomplete"`
       - `reasons=["daemon_unresponsive"]`
       - busy-session fix text
  3. `oneshot.rs`
     - a response-result test proving that a structured busy daemon error becomes the expected shell payload

#### Replace Phase 1
- Current tree has:
  - args/help for `replace`
  - fix-text assertions that recommend `replace`
- Current tree does **not** have:
  - a behavior test proving `replace` actually changes runtime behavior
- Best first tests if Option A is chosen:
  1. `oneshot.rs`
     - when `args.replace == true` and the session is active, `start_browser(...)` takes the stop+start path instead of returning `already_running=true`
  2. installed-binary smoke
     - active session + `browser start --replace`
     - verify a new daemon start actually occurs
     - verify the resulting runtime is healthy
  3. doc/help assertions
     - update wording so tests no longer imply "clean state reset" unless that behavior is truly implemented

### Recommended first implementation slices

#### Slice A: Busy truthfulness only
- Files:
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/protocol.rs`
  - minimal `oneshot.rs` tests if needed
- Outcome:
  - ordinary commands on busy sessions stop surfacing bare `daemon_transient`
  - shell callers get `session/state/reasons/fix`
- Why this slice is good:
  - high leverage
  - bounded surface
  - no daemon concurrency rewrite yet

#### Slice B: Replace contract truthfulness only
- Files:
  - `rust/src/cli/oneshot.rs`
  - `rust/src/cli/args.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Outcome:
  - either:
    - `--replace` works as "restart same named session runtime, preserve profile"
    - or it stops being suggested publicly
- Why this slice is good:
  - removes the sharpest contract lie quickly
  - does not require waiting for full browser-child cleanup work

### First-pass implementation proposals

#### Proposal A: Busy Slice A

##### Exact code targets
1. `rust/src/cli/connection.rs`
   - `send_request_existing(...)`
   - `send_request_with_retry(...)` or a thin wrapper around it
   - `session_target_state(...)`
2. `rust/src/cli/protocol.rs`
   - `openpage_error_from_structured_context(...)`
   - `openpage_error_context(...)`
   - associated round-trip tests
3. `rust/src/cli/oneshot.rs`
   - response-result tests only if needed

##### Recommended minimal design
- Keep transport retry behavior as-is.
- After retry exhaustion for an existing session request:
  - probe `daemon_status(session)?`
  - probe `session_target_state(&status)?`
  - if `SessionTargetState::Unresponsive`, return a structured browser/session error instead of `daemon_transient`
- Recommended public shape for that first version:
  - `error.kind = "browser_operation"`
  - `error.session = <session>`
  - `error.state = "incomplete"`
  - `error.reasons = ["daemon_unresponsive"]`
  - `error.fix = daemon_unresponsive_fix(...)`
- Recommendation is to build that error with structured context, not by handcrafting a special free-form message.

##### Why this is the best first cut
- touches the highest-leverage shared request path
- avoids a brand-new public error kind for now
- reuses the state taxonomy already present in:
  - `browser status`
  - `browser list`
  - `doctor`

##### Tests to add first
1. `connection.rs`
   - a targeted test for "retry exhaustion + runtime probe says Unresponsive -> structured busy browser error"
2. `protocol.rs`
   - round-trip reconstruction of:
     - kind `browser_operation`
     - state `incomplete`
     - reasons `["daemon_unresponsive"]`
3. `oneshot.rs`
   - a response-result/shell-payload test that asserts busy commands no longer only expose generic retry guidance

##### What this proposal intentionally does not solve
- it does not make the daemon more concurrent
- it does not make `stop` fast under a monopolized daemon
- it is truthfulness and UX correctness, not root-cause daemon scheduling repair

#### Proposal B: Replace Slice B

##### Exact code targets
1. `rust/src/cli/oneshot.rs`
   - `start_browser(args: BrowserStartArgs)`
   - maybe a small helper such as `prepare_replace_session(session, replace)`
2. `rust/src/cli/args.rs`
   - keep the flag, but update wording if semantics change
3. Docs/fix text:
   - `README.md`
   - `skills/openpage-test/references/session-management.md`
   - `skills/openpage-test/references/cli-smoke.md`
   - `rust/src/cli/protocol.rs`
   - `rust/src/cli/connection.rs`

##### Recommended minimal design
- If `args.replace`:
  - call `stop_browser_session(&args.session, true)?` before `webpage.create`
  - continue through the existing start path
- Keep profile continuity:
  - same named session
  - same `OPENPAGE_HOME/profiles/<session>`
- Reword public contract to mean:
  - "restart this named session runtime before continuing"
  - not "clear browser state for this session"

##### Why this is the best first cut
- smallest real implementation that makes the flag truthful
- no daemon protocol change required
- consistent with current persistent-profile session design

##### Tests to add first
1. `oneshot.rs`
   - a new test around the replace path that proves active-session `--replace` does not fall through to plain `already_running=true`
2. runtime smoke
   - active session -> `browser start --replace`
   - verify the command re-establishes a healthy runtime
3. wording tests/docs
   - remove "clean restart" style language unless fresh-state behavior is truly implemented

##### Known limitation of this proposal
- it still inherits current forced-stop cleanup weaknesses
- so it makes `--replace` truthful at the CLI contract level, but not yet maximally reliable under daemon-busy/orphan-Chrome edge cases

### Busy Slice A feasibility spike (2026-06-06, latest pass)

- Intent:
  - verify that the top-ranked project really can land as a narrow truthfulness slice before any larger daemon refactor
- Code changes in the spike:
  - `rust/src/cli/connection.rs`
    - `send_request_existing(...)` now remaps exhausted transient existing-session failures through a narrow helper
    - when the daemon still looks alive/ready but `session_target_state(...)` probes as `Unresponsive`, the helper returns a structured busy browser/session error instead of generic `daemon_transient`
  - `rust/src/cli/protocol.rs`
    - structured reconstruction now recognizes the canonical busy message:
      - `session \`...\` is currently busy or unresponsive`
    - `openpage_error_fix(...)` now preserves fix text for that canonical busy-session state too
- New focused tests:
  - `cli::connection::tests::remap_existing_session_request_error_uses_busy_state_for_unresponsive_session`
  - `cli::protocol::tests::reconstructs_openpage_error_from_structured_context_for_busy_incomplete_state`
- Verification:
  - targeted test:
    - `cargo test --manifest-path rust/Cargo.toml remap_existing_session_request_error_uses_busy_state_for_unresponsive_session -- --nocapture`
  - targeted test:
    - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_busy_incomplete_state -- --nocapture`
  - compile:
    - `cargo check --manifest-path rust/Cargo.toml`
- What this spike confirms
  - the top-ranked project really does have a small, shared entry point
  - preserving `session/state/reasons/fix` for busy ordinary commands is possible without first rewriting daemon concurrency
  - protocol round-trip support was indeed the hidden dependency; once patched, the slice holds together cleanly at test level
- What this spike does not prove yet
  - end-to-end installed-binary user behavior has not been re-dogfooded after the code change
  - stop latency is unchanged
  - daemon monopolization/root-cause scheduling is unchanged

#### Project 3: Batch readability and composition UX
- User problem:
  - NDJSON is machine-friendly but hard for humans to correlate with commands in mixed success/failure runs
- Why this is a project:
  - this is a contained CLI UX surface with low architectural coupling
- Likely files:
  - `rust/src/cli/oneshot.rs`
  - `rust/src/cli/args.rs`
  - `README.md`
- Verification criteria:
  - each output line carries enough context to map result -> command without guesswork
  - `--bail` makes it obvious which command stopped the run

## User requirements (this run)
- Use exactly one stable config system: `config.toml`.
- Do not keep ini fallback, including dp-style fallback.
- Provide mature precedence: user-level, workspace-level, env-level, and CLI override.
- Browser executable lookup should follow explicit path first, then cross-platform discovery.

## Current facts from code
- Active runtime still loads ini defaults in CLI path:
  - `rust/src/cli/serve.rs`: `LaunchOptions::from_ini(None)`
  - `rust/src/cli/serve.rs`: `SessionOptions::from_ini(None)`
  - `rust/src/cli/doctor.rs`: `LaunchOptions::from_ini(None)`
- ini resolution still points to:
  - `rust/configs.ini`
  - `./dp_configs.ini`

## Design decisions (this run)
- Add a dedicated `rust/src/config.rs`.
- `config.toml` locations:
  - user: `OPENPAGE_HOME/config.toml` (default `~/.openpage/config.toml`)
  - workspace: `<cwd>/.openpage/config.toml`
- Precedence:
  - `CLI > ENV > workspace > user > built-in defaults`
- Remove ini fallback from active CLI runtime paths; keep changes surgical to config-related files.

## Verification notes
- `cargo check` passes after refactor.
- Full `cargo test` cannot currently run because of a pre-existing unrelated test compile error in `rust/src/download.rs` (`Arc::clone` on `Mutex` field in test-only code).
- Runtime verification with `cargo run --bin openpage -- doctor --quick` confirms:
  - browser config source now reports `default | user config.toml | workspace config.toml | OPENPAGE_BROWSER_PATH`
  - no active doctor/runtime output references `rust/configs.ini` or `dp_configs.ini`

# Notes: openpage build log

## Current repository facts
- Root now contains a real `openpage` project with:
  - `rust/`
  - `python/`
  - `scripts/`
  - planning/docs files
- Git repository has been initialized locally.
- Toolchain present:
  - `rustc 1.94.1`
  - `cargo 1.94.1`
  - `Python 3.14.4`
  - `uv 0.9.24`

## Reference project observations
- Reference API shape is centered around:
  - `ChromiumPage`
  - `SessionPage`
  - `WebPage`
  - `ChromiumOptions`
  - `SessionOptions`
- Strongest Rust replacement candidates are:
  - CDP transport and connection lifecycle
  - event dispatch
  - network listener
  - download manager
- Python-specific semantics worth preserving selectively:
  - Page / Element object model
  - convenience wrappers
  - configuration ergonomics

## Constraints from user
- Root must contain `python/` and `rust/`
- Python should use locally built Rust artifacts
- Result should be directly runnable and verifiable without further user input
- Do not stop early; keep auditing completion against concrete evidence

## Working direction
- Create a Rust crate that exposes a PyO3 extension module.
- Make Python package import the extension and provide a thin API.
- Preserve key names inspired by the reference project while not pretending to fully reimplement all of DrissionPage in one pass.

## Architecture conclusions from parallel research
- `chromiumoxide` is the most pragmatic current backbone for a Rust-owned Chromium/CDP core.
- `cdp-protocol` is the right long-term direction if `openpage` later wants to own more of the protocol and transport stack.
- First release should be browser-first, not `WebPage`-first.
- `WebPage` was not the right first Rust-native boundary, but it is now implemented in Rust on top of the stabilized browser/session primitives.
- `SessionElement` is semantically a snapshot object, not a live handle.
- `WebPage` compatibility floor is mode switching plus current-context cookie sync.
- The crate can now compile as a pure Rust library without `pyo3`; Python bindings are feature-gated behind `python-module`.

## Current implemented surface
- Rust core:
  - `LaunchOptions`
  - `Browser`
  - `Page`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage`
  - locator parsing for CSS / `tag:` / `t:` / `@name=value` / `xpath:`
  - browser/session cookie header transfer primitives
  - browser-backed browser/page/element state checks and wait polling
  - page-scoped network listener with Rust-owned packet queueing, filter matching, and response body capture
  - request/response extra info exposure through the same Rust listener core
  - browser download-path configuration, mission tracking, cancel/wait support, and wait-for-download helper
- Python wrappers:
  - `ChromiumOptions`
  - `Browser`
  - `Page`
  - `ChromiumPage`
  - `Element`
  - `SessionOptions`
  - `SessionPage`
  - `SessionElement`
  - `WebPage` thin wrapper over the Rust core
  - `Listener` / `ListenerPacket` thin wrappers over the Rust listener core
  - `DownloadMission` thin wrapper over the Rust download tracker
- Verified operations:
  - launch browser
  - open page
  - read `url`, `title`, `html`
  - read browser/session `user_agent`
  - read session-backed `status_code` from `WebPage` in session mode
  - read browser/session/`WebPage` `cookies()`
  - read session-backed `raw_data` and `encoding`
  - read browser/page/element states from Rust-backed browser objects
  - wait for browser new-tab/download begin/download done from Rust
  - wait for page title/url/load changes, locator presence, and element displayed/hidden/enabled/deleted/clickable states from Rust
  - download a local file through a Rust-configured browser download path and wait for it from Rust
  - capture completed browser network packets from both `ChromiumPage` and driver-mode `WebPage`
  - capture listener response bodies for matched browser requests
  - capture listener response extra info for matched browser requests
  - capture browser download missions from both `ChromiumPage` and driver-mode `WebPage`
  - query elements
  - nested snapshot queries from browser/session/html snapshots
  - snapshot root lookup plus `child / children / parent / prev / next / before / after / prevs / nexts / befores / afters`
  - snapshot node metadata `tag / inner_html / raw_text / attrs`
  - input, click, clear
  - run JS
  - screenshot
  - PDF save API
  - tab ids / count lookup
  - session HTML fetch and JSON fetch
  - `WebPage` browser -> session cookie sync
  - `WebPage` session -> browser cookie sync
  - Python bindings detach from the interpreter during blocking Rust work

## Verified commands
- `cargo check` in `rust/`
- `cargo test` in `rust/`
- `cargo check --features python-module` in `rust/`
- `bash scripts/dev_install.sh`
- `bash scripts/run_checks.sh`
- `python/.venv/bin/python python/examples/basic_usage.py`
- `python/.venv/bin/python python/examples/webpage_modes.py`
- `cargo run --manifest-path rust/Cargo.toml --example webpage_modes`

## Next audit focus
- The snapshot DOM core now covers the main relative-navigation family in Rust.
- The next parity gap inside this area is the remaining reference-style helpers around paths, comments, and richer locator modes.
- `cookies()` has now moved into the Rust core and Python only adapts the returned objects.
- Session response metadata `raw_data` and `encoding` now lives in Rust as well.
- Basic browser download enablement and wait-for-download now lives in Rust too.
- Browser-backed wait/state has a broader Rust-owned pass now, but it still lacks the broader reference-style surface such as event-driven document-load orchestration, richer element wait variants, and stronger new-tab tracking semantics.
- Listener now has response body capture and extra-info merging in Rust, but it still lacks fuller reference-style parity such as interception controls.
- Download management now has a first Rust-owned pass as well, but it still lacks richer reference-style policies such as rename/skip/overwrite coordination and broader per-tab controls.
- The next highest-value surfaces are fuller listener/download parity, then the remaining reference-style convenience and parity helpers.

## Protocol migration audit (2026-05-29)
- Current CLI reality was originally split across three paths:
  1. `serve --stdio`
  2. `serve --port`
  3. `oneshot.rs` direct browser/session control
- `rust/src/cli/protocol.rs` is already a reasonable NDJSON protocol boundary; the problem is not schema shape, but that the CLI does not consistently route through it.
- `serve --stdio` has now been removed from active code and active docs/skills; TCP daemon is the only public daemon protocol path.
- `rust/src/cli/oneshot.rs` still performs the real work for most commands through `load_session()`, `open_page()`, and `Browser::connect()`, so the daemon is not yet the single source of execution truth.
- First migrated daemon-backed CLI commands now include:
  - `browser start/stop/status`
  - `goto`, `url`, `title`, `html`
  - `snapshot`, `screenshot`
  - `click`, `fill`, `focus`, `clear`, `submit`, `check`, `uncheck`, `text`, `attr`
- Second migrated daemon-backed CLI commands now include:
  - `scroll`
  - `back`, `forward`, `reload`, `stop-loading`
  - `right-click`, `middle-click`, `double-click`
  - `key-down`, `key-up`, `shortcut`, `input`, `type`, `type-with-interval`
  - `js`, `download`
  - `intercept start/stop/status`
  - `alert accept/dismiss/text`
  - `scroll-into-view`, `hover`, `press`, `select`, `upload`
  - `drag`, `drag-to`, `drag-to-point`
  - `active-element`
  - `wait-for-url`, `wait-for-title`, `wait-for-function`, `wait-for-text`
  - `pdf`
  - `storage get/set`
  - `cookies get/set/delete/clear`
- `rust/src/cli/connection.rs` has now absorbed more of the non-CDP `agent-browser` borrowing target:
  - daemon version mismatch restart
  - startup log capture to `OPENPAGE_HOME/daemon/<session>.log`
  - broader transient error retry classification
- AI-first ref flow is now working through the daemon path:
  - `snapshot` assigns `data-op-ref`
  - CLI `click @e1` is normalized to `[data-op-ref="e1"]`
  - verified end-to-end against a data URL page
- Remaining direct-path concentration is now much narrower:
  - `drag-in`
  - generic `wait`
  - `click-to-download`
  - `click-to-upload`
  - `click-for-new-tab`
  - `tab *`
  - `frame *`
- Best near-term borrow from `agent-browser` is the daemon infrastructure only:
  - sidecar files
  - daemon discovery
  - startup/retry/timeout handling
  - graceful shutdown / idle timeout
- We should not borrow:
  - competitor CDP transport
  - competitor action dispatch layer
  - competitor element interaction internals

## Protocol migration audit update (2026-05-29, later pass)
- `rust/src/cli/serve.rs` mid-flight refactor has been completed enough to compile again.
- `ServeWebPage` is now the daemon-side holder for:
  - active `WebPage`
  - active frame target
- Daemon-side context-sensitive operations now cover:
  - `tab.list`
  - `tab.new`
  - `tab.switch`
  - `tab.close`
  - `frame.list`
  - `frame.switch`
  - `wait.locator`
  - `page.drag_in`
  - `element.click_to_download`
  - `element.click_to_upload`
  - `element.click_for_new_tab`
- `rust/src/cli/oneshot.rs` no longer contains active execution references to:
  - `open_page()`
  - `load_session()`
  - `Browser::connect()`
  - `do_start_browser()`

## Control-plane contract consistency sweep (2026-06-05)

- This pass intentionally did not add new shell fields. It was a closure audit.
- Cross-checked these files against each other:
  - `README.md`
  - `doctor-契约盘点-v1.md`
  - `doctor-契约收口结论-v1.md`
  - `browser-daemon-契约盘点-v1.md`
  - `控制面总览-契约关系-v1.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- No implementation/doc contradiction found on the current stable shell contract points:
  - `kind="daemon_session"`
  - `fixable_ids` semantics
  - `fixed[] source / reason`
  - post-fix view semantics
  - `log_path / log_exists` vs `path / exists` compatibility alias wording
- Refreshed `竞品文档-考虑借鉴的部分v1.md` so it now explicitly states which competitor ideas have already been absorbed into OpenPage.
- Re-ran focused regression checks:
  - `production_check_kinds_match_documented_stable_set`
  - `browser_logs_payload_backfills_daemon_session_kind_when_missing`
  - `doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions`
- Result:
  - current remaining work should stay in “small shell/control-plane tightening” mode
  - there is no evidence from this pass that we need a broader contract rewrite

## Removed-surface parse-error migration fixes (2026-06-05)

- This pass tightened a real shell-contract gap rather than adding new control-plane fields.
- Before this change:
  - removed `page ...` and removed `serve --stdio` already returned `error.kind=\"invalid_input\"`
  - but the payload still depended on free-form clap text for migration guidance
- Now:
  - direct parse failures for removed `page ...` carry `error.fix`
  - direct parse failures for removed `serve --stdio` carry `error.fix`
  - nested batch parse failures for the same removed surfaces carry the same `error.fix`
- Implementation shape:
  - `cli/protocol.rs::known_invalid_input_fix(...)` is the single helper
  - `cli/mod.rs::clap_error_payload(...)` and `cli/oneshot.rs::batch_error_payload(...)` both reuse it
- Verified with:
  - focused unit tests for direct parse errors
  - focused unit tests for batch nested parse errors
  - real CLI smoke for:
    - `openpage page url`
    - `openpage serve --stdio`
- Result:
  - direct CLI parse errors and batch nested parse errors are now more aligned with the broader “structured fix guidance” discipline already used by daemon/state failures

## Batch workflow-restriction fixes (2026-06-05)

- This pass extended the same structured-fix discipline to one narrow `unsupported_operation` family.
- Covered only known batch workflow restrictions with a clear top-level recovery path:
  - `batch cannot execute serve`
  - `batch cannot execute doctor`
  - `batch cannot execute nested batch commands`
- Important non-goal:
  - do not auto-attach `fix` to every `unsupported_operation`
  - platform-specific cases like `downloads open is unsupported on this platform` still omit `fix`
- Implementation shape:
  - `cli/protocol.rs::known_unsupported_operation_fix(...)`
  - `UnsupportedOperation` now participates in `openpage_error_context(...)`
- Verified with:
  - focused protocol tests
  - focused batch payload tests
  - real CLI smoke for restricted batch commands
- Result:
  - `unsupported_operation` still means “not supported in this workflow/platform”
  - but known workflow restrictions no longer force callers to scrape the message to learn the correct top-level recovery path

## Session-local unsupported-operation fix: tab reopen prereq (2026-06-05)

- This pass extended the same pattern to one session-local prerequisite failure:
  - `no recently closed tab recorded for this session`
- Reason this is a good fit:
  - it is still `unsupported_operation`, not `invalid_input`
  - but there is a clear next step:
    - close a tab first, then use `tab reopen`
    - or fall back to `tab new <url>`
- Important boundary:
  - I did not broaden the rule to arbitrary session-local restrictions
  - this remains an explicit allowlist inside `known_unsupported_operation_fix(...)`
- Verification stayed at protocol level plus `cargo check`
  - reproducing the exact CLI/runtime failure deterministically would require live browser/session setup and recent-tab stack manipulation
- Result:
  - the known `tab reopen` prerequisite failure now participates in the same machine-readable `error.fix` discipline as the batch workflow restrictions

## drag-in missing-payload fix alignment (2026-06-05)

- This pass targeted a dual-surface validation case:
  - direct CLI emits `drag-in requires --text or --files`
  - daemon-side validation emits `drag-in requires text or files`
- Reason this was worth closing:
  - both are really the same user mistake
  - before this pass, they already converged to `invalid_input`, but not to the same structured `fix`
- Implementation shape:
  - `known_invalid_input_fix(...)` now recognizes both string variants
  - `UnsupportedOperation` fix lookup now also falls back to the invalid-input fix allowlist
- Verification:
  - focused protocol tests for both direct-shell and daemon-response paths
  - `cargo check`
  - real CLI smoke for the direct-shell path:
    - `openpage drag-in '#dropzone' --session smoke-drag-fix`
- Result:
  - this is a better example of shell/control-plane alignment than just adding one more isolated fix, because it explicitly keeps a direct CLI validation string and a daemon-side validation string in sync

## Enum-style invalid-input fixes: snapshot/select (2026-06-05)

- This pass covered a small family of “finite allowed choices” errors:
  - `select requires one of: text, value, index`
  - `unsupported snapshot mode: ...`
  - `unsupported snapshot format: ...`
- Important implementation detail:
  - `known_invalid_input_fix(...)` already existed
  - but `BrowserOperation` was not yet consulting it unless the error happened to be a session-state sentence
  - that gap is now closed with a fallback lookup inside `openpage_error_context(...)`
- Why this matters:
  - previously, `UnsupportedOperation -> invalid_input` cases could get fix hints while some `BrowserOperation -> invalid_input` cases could not
  - now the fix lookup follows the invalid-input semantics more consistently, not just the original enum variant
- Verification stayed at protocol tests plus `cargo check`
  - no live runtime smoke here because these are daemon/session-backed validation cases rather than parse-only shell failures
- Result:
  - `error.fix` is now more aligned to the *meaning* of the error bucket (`invalid_input`) than to the historical Rust enum variant that carried the detail string
- Verified by grep:
  - `rg -n "open_page\\(|load_session\\(|Browser::connect\\(|do_start_browser\\(" rust/src/cli`
  - returns no matches
- Real smoke that passed through daemon path:
  - generic `wait '#upload'`
  - `frame list`
  - `frame switch 1`
  - frame-scoped `text '#inside'`
  - `frame switch main`
  - `click-for-new-tab '#newtab'`
  - `tab switch 2`
  - `click-to-upload '#upload'`
  - `js 'document.querySelector(\"#upload\").files.length'`
  - `click-to-download '#download'`
  - `drag-in '#drop' --text 'Dragged text'`
  - `js 'document.body.dataset.dropResult'`
  - `tab close --others`
  - `tab list`
  - `browser stop`
- Important nuance from smoke:
  - `click-for-new-tab` correctly switched active tab inside daemon state.
  - The first upload smoke failed only because the test stayed on the new tab; after explicit `tab switch 2`, upload/download/drag-in all passed.

## Borrowed non-CDP design audit update (2026-05-29, output governance pass)
- `rust/src/cli/protocol.rs` already contained a first pass of output governance helpers:
  - `OPENPAGE_CONTENT_BOUNDARIES`
  - `OPENPAGE_MAX_OUTPUT_CHARS`
  - top-level filtering only for `result.html` / `result.text` / `result.value`
- Before this pass, the helpers were effectively dead code because:
  - `rust/src/cli/oneshot.rs::print_json()` still called raw `serde_json::to_string`
  - `rust/src/cli/mod.rs` top-level error printing also bypassed the formatter
- This pass wired both output exits to `format_output_json()`:
  - normal JSON results now pass through the same filter path
  - top-level CLI errors also pass through the same serializer, though they are not boundary-wrapped because they do not contain `result.*` payloads
- Real smoke verification with `OPENPAGE_HOME=/tmp/openpage-output-governance-smoke`:
  - `browser start data:text/html,... --session out1 --headless`
  - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=40 openpage html --session out1`
  - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=12 openpage text '#a' --session out1`
  - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=12 openpage text '#missing' --session out1`
  - `browser stop --session out1`
- Observed behavior:
  - `html` output now carries `_boundary` metadata and wrapped/truncated content
  - `text` output now carries `_boundary` metadata and wrapped/truncated content
  - error JSON remains valid and unwrapped, which is the intended behavior for payloads without `result.html/text/value`

## Compile blocker found during output-governance verification
- `cargo check --manifest-path rust/Cargo.toml` initially failed in `rust/src/page.rs`
- Failure:
  - `CaptureSnapshot` result was being mapped with `.map(|result| result.data)`
  - current type shape only allowed borrowing there, so moving the `String` out failed
- Minimal fix applied:
  - `.map(|result| result.data.clone())`
- This was not part of the protocol design work itself, but it was required to restore a verifiable compile state for the current worktree

## Borrowed non-CDP design audit update (2026-05-29, doctor local-path hint pass)
- Current machine facts re-verified:
  - `OPENPAGE_HOME=/Users/yuuu/.openpage`
  - healthy daemon sessions:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `human-flow`
  - repo config still loads `browser_path=chrome` from `rust/configs.ini`
  - this machine does **not** resolve `chrome` on PATH
  - this machine **does** have:
    - `/Applications/Google Chrome.app`
    - `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Code change made in `rust/src/cli/doctor.rs`:
  - when configured `browser_path` is missing, doctor now probes common local browser candidates
  - for the current macOS machine, `doctor --quick` now returns:
    - the existing `browser.executable` failure
    - plus an explicit `browser.executable.hint`
    - with the exact local path that should work
- This is intentionally only a **diagnostic / outer-shell** improvement:
  - it does not change OpenPage browser/CDP/element internals
  - it does not hardcode repo config to a macOS-specific absolute path
  - it keeps the current config problem visible instead of silently masking it
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml suggested_browser_executable -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml missing_browser_message_includes_hint_when_present -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`

## Historical-doc noise reduction update (2026-05-29)
- Two high-risk historical docs were left in place but explicitly downgraded at the top:
  - `rust_progress_report.md`
  - `协议迁移审计-v1.md`
- Reason:
  - both still contain large amounts of now-obsolete truth such as:
    - `serve --stdio`
    - one-shot attach as main path
    - old `page get/page url/page title/page screenshot`
  - deleting or fully rewriting them this turn would be broader than necessary
  - a prominent historical banner prevents future sessions from mistaking them for current authority

## Borrowed non-CDP design audit update (2026-05-29, batch pass)
- `agent-browser`'s next most transferable outer-shell feature after output governance was `batch`
- OpenPage's current clap-based CLI has no global `Flags` layer, which simplified one design decision:
  - no need to preserve global JSON/verbosity/session semantics from the competitor
  - batch can be implemented as a plain subcommand that reuses existing per-command clap parsing
- Current OpenPage batch shape:
  - new `Command::Batch(BatchArgs)` in `rust/src/cli/args.rs`
  - `BatchArgs` fields:
    - `--bail`
    - `commands: Vec<String>` for argument mode
  - when `commands` is empty, stdin is read as JSON `Vec<Vec<String>>`
- Argument mode uses `shlex::split(...)` to mirror competitor-style quoted-command behavior
- Batch intentionally refuses:
  - nested `batch`
  - nested `serve`
- Reason:
  - keeps the execution model simple
  - prevents batch from spawning daemon listeners or recursively recursing through command groups
- One small outer-shell refactor was required:
  - `rust/src/cli/oneshot.rs::run(...)` now returns an explicit exit code
  - this avoids printing per-command error JSON and then also printing an extra top-level aggregate error JSON
- Real smoke verification:
  - argument mode:
    - `batch "browser start data:text/html,... --headless" "title" "browser stop"`
  - stdin mode:
    - `printf '[[...],[...],[...]]' | openpage batch`
  - bail mode:
    - `batch --bail "serve --port 0" "browser list"`
- Observed behavior:
  - argument mode ran 3 commands sequentially and returned the expected title
  - stdin mode ran 3 commands sequentially and returned the expected title
  - `--bail` stopped on the first unsupported command and returned exit code 1
  - no old direct CLI browser execution path reappeared; the old-path grep remained empty

## CLI help vs README audit (2026-05-29, post-batch)
- `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help` now shows:
  - `batch`
  - `browser`
  - the full TCP daemon-backed command surface
- README currently already mentions:
  - `browser list`
  - `browser status`
- README does **not yet** mention:
  - `batch`
  - `OPENPAGE_CONTENT_BOUNDARIES`
  - `OPENPAGE_MAX_OUTPUT_CHARS`
- Conclusion:
  - code/help is ahead of README
  - next doc pass should sync README to the current CLI surface before marking verification complete

## Borrowed non-CDP design audit update (2026-05-29, doctor pass)
- OpenPage now has a minimal `doctor` command as a borrowed outer-shell design
- Scope intentionally kept small:
  - `doctor --quick`
  - `doctor`
  - JSON-only output, matching the current CLI's machine-oriented style
  - no destructive `--fix` path yet
- Checks implemented:
  - Environment:
    - `OPENPAGE_HOME`
    - daemon sidecar directory
  - Daemon:
    - sidecar session discovery without cleanup
    - per-session `alive/ready/port/pid/version` via existing `daemon_status()`
  - Browser:
    - config load through `LaunchOptions::from_ini(None)`
    - optional live headless launch smoke through the existing `LaunchOptions` + `Browser::launch` path
- Batch intentionally rejects nested `doctor`, just like nested `serve`
- Real local findings from `doctor --quick` / `doctor`:
  - `OPENPAGE_HOME` currently resolves to `/Users/yuuu/.openpage`
  - daemon sidecars currently live in `/Users/yuuu/.openpage/daemon`
  - currently observed live sessions:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `human-flow`
  - launch config currently loads from:
    - `/Volumes/data0/data4work/2026_5/openpage/rust/configs.ini`
  - loaded browser config currently says:
    - `browser_path=chrome`
    - `headless=false`
    - `auto_port=false`
  - doctor now resolves browser executables before launch:
    - current result: configured executable `chrome` was not found
  - `doctor --quick` therefore now fails at the config/executable layer already
  - full `doctor` skips live launch after that failure instead of emitting a second redundant launch error
- Interpretation:
  - the CLI/daemon path is healthy
  - the current local browser-launch issue is environmental/configurational, not a protocol-regression signal

## Borrowed non-CDP design audit update (2026-05-29, daemon inventory + doctor integration pass)
- `rust/src/cli/connection.rs` already had a richer daemon-sidecar model in progress:
  - `DaemonInventory`
  - `IncompleteDaemonSession`
  - `CleanedDaemonSession`
  - `daemon_inventory()`
- Before this pass, `rust/src/cli/doctor.rs` was still using its own older `discover_daemon_sessions()` scan and `daemon_status()` loop:
  - no visibility into incomplete alive sessions
  - no visibility into cleaned stale sidecars
  - duplication between doctor and connection layer
- This pass connected `doctor` to `daemon_inventory()` directly:
  - healthy sessions now render as `daemon.session.*`
  - alive but incomplete sidecars now render as `daemon.incomplete.*`
  - dead/stale sidecars cleaned during the scan now render as `daemon.cleaned.*`
  - old `discover_daemon_sessions()` was removed from `doctor.rs`
- Local verification after the change:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor`
- Current local facts re-confirmed:
  - `OPENPAGE_HOME=/Users/yuuu/.openpage`
  - daemon sidecars currently live in `/Users/yuuu/.openpage/daemon`
  - healthy sessions currently observed:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `human-flow`
  - all 3 currently report:
    - `alive=true`
    - `ready=true`
    - `version 0.1.0`
  - browser config still loads from:
    - `/Volumes/data0/data4work/2026_5/openpage/rust/configs.ini`
  - current failure remains:
    - configured executable `chrome` not found on this machine
- Synthetic verification with temporary `OPENPAGE_HOME`:
  - cleaned path:
    - wrote invalid `.pid` + invalid `.port` + valid `.version`
    - `doctor --quick` emitted `daemon.cleaned.dead`
    - sidecar files were removed afterward
  - incomplete path:
    - started a real daemon in a temp home
    - removed only its `.version`
    - `doctor --quick` emitted `daemon.incomplete.doctor-live`
- One follow-up command intended to re-run the incomplete-alive smoke hit a cargo artifact lock race:
  - background `serve` was still compiling when `doctor` started
  - the resulting output was discarded as invalid evidence
- Conclusion:
  - the TCP-only protocol path still holds
  - the borrowed daemon-inventory design is now materially integrated, not just staged in `connection.rs`
  - next valuable non-CDP work is docs/help/skills consistency and possibly surfacing richer inventory in more user-facing commands

## User-facing consistency audit update (2026-05-29, README + repo-local skills pass)
- After the daemon-inventory pass, the next highest-value inconsistency was in active user-facing docs and repo-local smoke scripts, not in the execution path itself.
- Findings before this pass:
  - `README.md` did not explicitly say that all user-facing CLI commands now share the same TCP daemon execution path
  - `skills/openpage-test/SKILL.md` still used "one-shot" wording as if it were a distinct control mode
  - `skills/openpage-test/scripts/oneshot_baidu_smoke.sh` still used obsolete command syntax:
    - `page get`
    - `page url`
    - `page title`
    - `page screenshot`
  - `skills/openpage-test/references/cli-smoke.md` still described `doctor --quick` as if it passed config checks on the current machine
- Changes made:
  - `README.md`
    - now explicitly states there is no separate stdio daemon mode or direct browser-execution path for the CLI surface
    - now describes `doctor` as reporting active healthy sessions, incomplete sidecars, and cleaned stale sidecars
  - `skills/openpage-test/SKILL.md`
    - replaced "one-shot" wording with:
      - raw TCP daemon control
      - named-session CLI commands
      - CLI-wrapper regression wording where appropriate
  - `skills/openpage-test/references/cli-smoke.md`
    - updated local `doctor --quick` reality
    - updated the named-session wording
    - clarified that current macOS machine has Chrome.app installed but `chrome` is not on PATH
  - `skills/openpage-test/references/install.md`
    - now mentions `OPENPAGE_BROWSER_PATH` and script auto-detection behavior
  - `skills/openpage-test/scripts/serve_baidu_smoke.sh`
    - now auto-detects browser path from:
      - `OPENPAGE_BROWSER_PATH`
      - common PATH names
      - macOS app bundle paths
    - forwards `browser_path` into `webpage.create`
  - `skills/openpage-test/scripts/oneshot_baidu_smoke.sh`
    - removed
  - `skills/openpage-test/scripts/named_session_baidu_smoke.sh`
    - added as the replacement
    - uses current real CLI commands:
      - `browser start`
      - `goto`
      - `url`
      - `title`
      - `screenshot`
- Local machine browser-path fact confirmed during this pass:
  - `chrome` not found on PATH
  - `/Applications/Google Chrome.app` exists
  - effective executable used by the updated smoke script:
    - `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Real runtime verification after the pass:
  - `bash skills/openpage-test/scripts/named_session_baidu_smoke.sh /tmp/openpage-cli-artifacts/review-baidu.png`
  - `bash skills/openpage-test/scripts/serve_baidu_smoke.sh /tmp/openpage-cli-artifacts/serve-baidu.png`
  - both succeeded
  - resulting files:
    - `/tmp/openpage-cli-artifacts/review-baidu.png`
    - `/tmp/openpage-cli-artifacts/serve-baidu.png`
  - `file` output showed both are valid PNG screenshots
  - visual inspection of `/tmp/openpage-cli-artifacts/review-baidu.png` confirmed visible Baidu homepage content, not a blank screenshot
- Interpretation:
  - current local browser-launch issue in `doctor` is specifically the default configured executable name, not a missing browser installation
  - repo-local smoke now compensates for that local config mismatch without changing OpenPage core launch logic
  - active repo-local docs now better reflect the single TCP execution path

## Borrowed non-CDP design audit update (2026-05-29, browser list inventory pass)
- After `doctor` started consuming `daemon_inventory()`, the remaining obvious gap was that the most direct user-facing inventory command, `browser list`, still exposed only healthy sessions.
- Before this pass:
  - `rust/src/cli/oneshot.rs` handled `BrowserCommand::List` with:
    - `list_daemons()?`
    - JSON result containing only `sessions`
  - that meant:
    - `browser list` hid alive-but-incomplete sidecars
    - `browser list` hid stale sidecars cleaned during the scan
    - users had to run `doctor` to see the richer inventory model
- This pass changed `BrowserCommand::List` to use `daemon_inventory()` directly and return:
  - `sessions`
  - `incomplete`
  - `cleaned`
- `rust/src/cli/args.rs` help text was updated from:
  - `List active daemon-backed browser sessions`
  - to:
  - `List daemon-backed browser sessions and sidecar audit state`
- `README.md` was updated accordingly so the command example no longer implies that `browser list` is only a session lister.
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
  - current local output:
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - result included:
      - `sessions`
      - `incomplete: []`
      - `cleaned: []`
  - synthetic verification with temporary `OPENPAGE_HOME`:
    - wrote invalid dead sidecars for `dead`
    - started a real daemon session `alive`
    - removed only `alive.version`
    - `browser list` returned:
      - `cleaned=[{session:\"dead\", reason:\"invalid pid, invalid port\"}]`
      - `incomplete=[{session:\"alive\", version_present:false, alive:true, ready:true, ...}]`
      - `sessions=[]`
- Interpretation:
  - the borrowed daemon-inventory design is now visible in both:
    - `doctor`
    - `browser list`
  - this is a better user-facing landing spot than keeping the feature only in diagnostics

## Borrowed non-CDP design audit update (2026-05-29, AI-first snapshot contract pass)
- Current OpenPage snapshot path before this pass:
  - daemon op `webpage.snapshot` returned only:
    - `{"snapshot": [...]}` from `agent_snapshot_script()`
  - that already supported the ref flow because the script stamped `data-op-ref=eN`
  - but the user-facing contract was still thinner than the borrowed competitor design:
    - no compact text summary
    - no explicit ref index object
    - no page context metadata
- This pass kept the existing internal mechanism unchanged:
  - same JS-based interactive-element scan
  - same `data-op-ref`
  - same locator normalization for `@eN`
- The pass only enhanced the CLI/daemon contract layer in `rust/src/cli/serve.rs`:
  - `snapshot`
  - `text`
  - `refs`
  - `origin`
  - `title` when available
  - `interactive_count`
- Important design choice:
  - the compact summary uses the key `text`
  - that means the already-borrowed output-governance path automatically applies:
    - `OPENPAGE_CONTENT_BOUNDARIES`
    - `OPENPAGE_MAX_OUTPUT_CHARS`
- Added narrow unit coverage for the new pure helpers:
  - `format_snapshot_text_includes_title_origin_refs_and_attrs`
  - `snapshot_refs_builds_ref_index`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml format_snapshot_text_includes_title_origin_refs_and_attrs -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml snapshot_refs_builds_ref_index -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - real CLI smoke with temporary `OPENPAGE_HOME` and explicit local browser path:
    - `browser start --session snap2 --replace --headless --browser-path /Applications/Google Chrome.app/Contents/MacOS/Google Chrome about:blank`
    - `js "document.body.innerHTML = ..."`
    - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=500 openpage snapshot --session snap2`
    - `click @e1 --session snap2`
    - `browser stop --session snap2`
- Observed snapshot output now included:
  - `_boundary`
  - `interactive_count: 3`
  - `origin: "about:blank"`
  - `refs`
  - raw `snapshot` array
  - `text` such as:
    - `@e1 [button] "Go" id="go"`
    - `@e2 [a] "More" href="https://example.com"`
    - `@e3 [input] placeholder="Email"`
- Important verification point:
  - `click @e1` still succeeded after the contract enhancement
  - so the borrowed AI-first snapshot design did not break the existing ref-action loop
- Interpretation:
  - this is a good example of “borrow outer-shell design, keep internal truth source”
  - no competitor CDP / snapshot tree / element model was imported
  - but the user-facing snapshot contract is now materially more agent-friendly

## Competitor borrow matrix update (2026-05-29, doc pass)
- Wrote root deliverable: `竞品文档-考虑借鉴的部分v1.md`
- The document now records, in one place:
  - current OpenPage local status against the 3 user constraints
  - file-level mapping from `agent-browser` borrow targets to OpenPage landing points
  - explicit allow / do-not-copy boundary
- Current highest-confidence “copy next” targets remain:
  - `参考项目/agent-browser-main/cli/src/output.rs` → stronger CSPRNG boundary nonce for `rust/src/cli/protocol.rs`
  - `参考项目/agent-browser-main/cli/src/doctor/mod.rs` and `cli/src/doctor/launch.rs` → richer `Check/fix/summary` structure for `rust/src/cli/doctor.rs`
  - `参考项目/agent-browser-main/skill-data/core/references/{snapshot-refs,session-management,trust-boundaries}.md` → OpenPage skill / agent-usage docs
- Explicit non-borrow boundary remains unchanged:
  - do not copy `参考项目/agent-browser-main/cli/src/native/*`
  - do not copy competitor CDP / element / interaction internals
  - do not let outer-shell borrowing leak into `rust/src/browser.rs` or page/element truth sources

## Local truth refresh (2026-05-29, post-doc sync)
- Re-verified current local state after the latest CLI/protocol edits:
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - returned 3 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - still returns exactly 1 fail:
      - `browser.executable`
    - the failure is the same local config mismatch as before:
      - `rust/configs.ini` currently points at `browser_path=chrome`
      - current machine does not resolve `chrome` on PATH
    - the doctor hint remains correct:
      - `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- `rust/src/cli/protocol.rs` is no longer in the “planned borrow” state for nonce generation:
  - current code already uses `getrandom`
  - nonce remains process-stable through `OnceLock`
  - boundary contract is therefore already aligned with the intended stronger output-trust design
- Consequence for next steps:
  - stop treating output nonce as pending work
  - move the next borrow focus to:
    - `rust/src/cli/doctor.rs` structure
    - OpenPage trust-boundary / snapshot-refs / session-management docs

## Local truth refresh (2026-05-30, agent-doc pass)
- Re-verified active user-facing surfaces only:
  - `README.md`
  - `rust/src/cli/*`
  - `skills/openpage-test/*`
- Result:
  - no active `serve --stdio` surface found
  - no active `open_page()` / `load_session()` / `save_session()` / CLI-side `Browser::connect()` execution path found
  - remaining old-protocol strings are confined to historical reports and notes, not the active CLI surface
- Re-ran current local checks on 2026-05-30:
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - returned 5 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - still returns exactly 1 fail:
      - `browser.executable`
    - current machine still resolves the fix hint to:
      - `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Borrowed docs now actually landed in the repo-local agent surface:
  - `skills/openpage-test/references/snapshot-refs.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/trust-boundaries.md`
- `skills/openpage-test/SKILL.md` now routes readers to those documents depending on:
  - ref-driven interaction tasks
  - multi-session workflows
  - secret / auth / prompt-injection-sensitive tasks

## Local truth refresh (2026-05-30, doctor-fix expansion pass)
- Rechecked doctor-focused runtime state after the new `fix` coverage landed:
  - `cargo test --manifest-path rust/Cargo.toml cli::doctor::tests -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - still fails only at `browser.executable`
    - now also includes a structured `fix` for the skipped `browser.launch` quick-mode item
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - latest run returned 4 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
- Important nuance:
  - the daemon session count changed during the same day from 5 to 4
  - therefore session count should be treated as runtime state, not a durable project fact
- Code-level borrow progress:
  - `rust/src/cli/doctor.rs`
    - `fix` coverage expanded into:
      - environment resolution
      - daemon inventory failures / incomplete sessions / no-session state
      - browser config load failures
      - browser launch skip/fail/warn cases
    - launch temp-dir cleanup is now handled through a small Drop guard
  - this is aligned with competitor outer-shell design and still does not touch:
    - browser core
    - element logic
    - CDP truth sources

## Local truth refresh (2026-05-30, daemon-session-fix pass)
- Follow-up doctor increment landed in `rust/src/cli/doctor.rs`:
  - `daemon.session.*` warning entries now derive a structured `fix`
  - current cases covered:
    - version mismatch → stop and restart session with current CLI
    - not-ready session → run `browser status`, inspect the daemon log, then stop/restart if stale
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_session_fix_prefers_version_restart_guidance -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_session_fix_points_to_status_and_log_when_not_ready -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
- Current runtime truth after this pass:
  - `doctor --quick` still has exactly one fail:
    - `browser.executable`
  - `browser list` still shows:
    - 4 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
- Interpretation:
  - no new protocol-path regression evidence
  - `doctor` is now materially closer to the competitor's structured diagnosis model while still staying fully outside the browser/CDP/element truth source

## Local truth refresh (2026-05-30, browser-launch-guard pass)
- Continued the doctor lifecycle cleanup line in `rust/src/cli/doctor.rs`:
  - launch smoke now uses `BrowserLaunchGuard`
  - current cleanup layers are:
    - best-effort `browser.close()` via guard Drop
    - temp-dir cleanup via Drop guard
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_launch_guard_without_browser_still_cleans_temp_dir -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml cli::doctor::tests -- --nocapture`
    - now 10 doctor-related tests pass
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - active-surface grep over:
    - `README.md`
    - `rust/src/cli/*`
    - `skills/openpage-test/*`
- Current conclusions:
  - still no evidence of active protocol-path regression
  - `doctor` launch cleanup is now less reliant on single explicit control-flow success paths
  - this is still fully outside the browser/CDP/element truth source

## Historical protocol doc downranking (2026-05-30)
- The remaining high-risk confusion source was not the active CLI surface, but two root historical docs:
  - `rust_progress_report.md`
  - `协议迁移审计-v1.md`
- This pass did not rewrite their historical body.
- Instead it made the archival status harder to miss:
  - title changed to `[ARCHIVED] ...`
  - added a `当前覆盖事实（2026-05-30）` block at the top
  - explicitly says:
    - current active CLI mental model is TCP-only
    - old commands in those files are historical samples
    - do not execute those commands against the current repo
    - use `doctor --quick`, `browser list`, and `skills/openpage-test/references/cli-smoke.md` instead
- Verification:
  - top-of-file inspection for both docs
  - active-surface grep still shows no live `serve --stdio` / old one-shot command surface in:
    - `README.md`
    - `rust/src/cli/*`
    - `skills/openpage-test/*`


## Local truth refresh (2026-05-30, stale-daemon uniqueness pass)
- This pass focused on the remaining gap in `rust/src/cli/connection.rs` rather than browser/CDP internals.
- Problem found in current code before the patch:
  - `ensure_daemon()` only killed an existing daemon when it was already `ready` but had a version mismatch.
  - If the old process for the same session was still alive but its TCP port never became reachable, the CLI could clean sidecars and spawn a replacement, leaving the old process behind.
  - That weakens the “one stable TCP daemon path per session” invariant.
- Landed code changes:
  - `rust/src/cli/connection.rs`
    - added `ExistingDaemonAction`
    - added `wait_for_daemon_ready(...)`
    - added `existing_daemon_action_with_retry(...)` / `existing_daemon_action(...)`
    - `ensure_daemon()` now:
      - reuses an existing matching ready daemon
      - kills version-mismatched ready daemons
      - gives an alive-but-not-ready daemon a short readiness grace period
      - kills it before respawn if it still never becomes reachable
  - `rust/src/cli/oneshot.rs`
    - removed the last old session-JSON residue:
      - `session_file()`
      - local `openpage_home()` helper
      - `browser stop` no longer deletes old `sessions/<name>.json`
    - stop flow is now only about TCP sidecars and `daemon.shutdown`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_ -- --nocapture`
    - 3 tests passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - passed
    - current runtime state: 5 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - still exactly 1 fail: `browser.executable`
    - current machine hint still points to `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Important nuance:
  - the healthy session count changed from 4 to 5 during the same day because `smoke-alert` exists now
  - this remains runtime state, not a durable repository fact
- Interpretation:
  - this is a real protocol-path hardening change, not just docs
  - it stays fully outside browser/CDP/element truth sources
  - it also removes one more old non-TCP residue from the active CLI path


## Local truth refresh (2026-05-30, origin-aware boundary pass)
- This pass continued the non-CDP outer-shell borrowing line in `rust/src/cli/protocol.rs`.
- Borrow target / intent:
  - competitor `output.rs` keeps trust boundaries tied to content origin
  - OpenPage already had nonce + key wrapping, but not origin-aware boundary metadata
- Landed code changes:
  - `rust/src/cli/protocol.rs`
    - `wrap_content(...)` now accepts `origin: Option<&str>`
    - wrapped page-content markers now include `origin=<url>` when available
    - `_boundary` metadata now includes:
      - `nonce`
      - `keys`
      - `origin`
  - added tests:
    - `format_output_json_includes_origin_in_boundary_metadata`
    - `format_output_json_omits_empty_origin_from_boundary_metadata`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_ -- --nocapture`
    - 2 tests passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - real CLI smoke with temporary `OPENPAGE_HOME` and explicit browser path:
    - `browser start about:blank --session boundary-smoke --replace --headless --browser-path /Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
    - `js "document.body.innerHTML = ..." --session boundary-smoke`
    - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=500 openpage snapshot --session boundary-smoke`
    - `browser stop --session boundary-smoke`
- Observed real snapshot output now included:
  - `_boundary.origin: "about:blank"`
  - wrapped `text` content beginning with:
    - `--- OPENPAGE_PAGE_CONTENT nonce=... key=text origin=about:blank ---`
- Interpretation:
  - this is a direct outer-shell trust-boundary improvement
  - it stays fully outside browser/CDP/element truth sources
  - it improves AI-facing snapshot/text output without replacing the existing internal snapshot mechanism


## Local truth refresh (2026-05-30, legacy-session-json doctor pass)
- This pass stayed on the CLI outer shell and targeted one concrete local residue of the old pre-TCP execution model.
- Current local evidence on this machine:
  - `~/.openpage/sessions/` still exists
  - current files include:
    - `cli-more-states-2.json`
    - `cli-state-queries.json`
    - `default.json`
    - `test.json`
- Important interpretation:
  - the active TCP daemon CLI path no longer reads these files
  - they are local legacy session artifacts, not part of the current execution truth
  - deleting user data automatically would be too destructive, so this pass adds diagnosis and explicit cleanup guidance instead of removing them silently
- Landed code changes:
  - `rust/src/cli/doctor.rs`
    - added `legacy_sessions_dir()`
    - added `legacy_session_files()`
    - `environment_checks(...)` now emits `env.legacy_sessions`
      - `pass` when no legacy JSON files exist
      - `warn` when old `sessions/*.json` files are found
      - includes a `fix` telling the user to back them up and remove the directory if no remaining workflow depends on them
  - added tests:
    - `legacy_session_files_returns_only_json_entries`
    - `legacy_session_files_returns_empty_when_directory_missing`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml legacy_session_files_ -- --nocapture`
    - 2 tests passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - current machine now reports:
      - `env.legacy_sessions` as `warn`
      - exactly 1 `fail` remains: `browser.executable`
      - summary now includes the legacy residue as a real warning rather than leaving it invisible
- Observed runtime nuance:
  - current healthy sessions changed again during the day and now include `smoke-dl` / `smoke-dl2` instead of the previous `smoke-alert` sample
  - session inventory remains runtime state, not a durable repository fact
- Interpretation:
  - this is another concrete move toward “one stable TCP path” because deprecated local artifacts are now visible to users and future agents
  - it still does not touch browser/CDP/element truth sources


## Local truth refresh (2026-05-30, doctor-summary pass)
- This pass continued the structured doctor-output line rather than touching protocol or browser internals.
- Problem before the patch:
  - `doctor` summary only exposed `pass / warn / fail`
  - but OpenPage doctor also uses `info` and `fix` heavily
  - any agent/script that wanted “how many informational checks were emitted?” or “how many checks are fixable?” had to rescan the whole `checks` array
- Landed code changes:
  - `rust/src/cli/doctor.rs`
    - `Summary` now includes:
      - `pass`
      - `warn`
      - `fail`
      - `info`
      - `fixable`
      - `total`
    - `summarize(...)` now counts all of those directly
  - added test:
    - `summarize_counts_info_fixable_and_total`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - current runtime summary is now:
      - `pass=9`
      - `warn=1`
      - `fail=1`
      - `info=2`
      - `fixable=3`
      - `total=13`
- Interpretation:
  - this is another non-CDP outer-shell improvement
  - it makes `doctor` materially easier for future agents/scripts to consume without reimplementing summary logic
  - it stays completely outside browser/CDP/element truth sources


## Local truth refresh (2026-05-30, post-summary doc audit)
- Re-audited root-level markdown files specifically for misleading **current OpenPage CLI protocol** wording.
- Result:
  - no new active user-facing OpenPage CLI handbook surface was found beyond what was already downgraded/corrected
  - remaining old protocol wording is still concentrated in:
    - archived historical reports (`rust_progress_report.md`, `协议迁移审计-v1.md`)
    - tracking files (`notes.md`, `task_plan.md`, `claude-progress.txt`)
    - comparative research docs that already carry correction notes (`dp_vs_openpage_comparison.md`)
  - competitor/reference-only docs inspected this pass do not currently need extra OpenPage protocol corrections:
    - `项目梳理与Rust替换分析.md`
    - `技术文档-竞品cli分析.md`
    - `竞品的流程.md`
- This means the remaining live work is still better spent in:
  - `doctor.rs`
  - `protocol.rs`
  - agent-facing docs
  rather than broad markdown churn.


## Local truth refresh (2026-05-30, active-doc sync pass)
- This pass updated only the active user-facing docs, not archived reports.
- Current local truth rechecked right before the doc edit:
  - `openpage browser list` returned 4 healthy sessions
  - `openpage doctor --quick` returned:
    - `warn=1` (`env.legacy_sessions`)
    - `fail=1` (`browser.executable`)
    - `info=2`
    - `fixable=3`
    - `total=11`
- Synced files:
  - `README.md`
    - `doctor` section now explicitly mentions legacy session JSON residue under `OPENPAGE_HOME/sessions`
    - boundary section now mentions origin-aware boundary metadata
  - `skills/openpage-test/references/cli-smoke.md`
    - latest local recheck now mentions:
      - 4 healthy sessions
      - legacy session JSON warning
      - browser executable as the remaining fail
  - `skills/openpage-test/references/trust-boundaries.md`
    - now explains that boundary metadata may carry origin, but that does not increase trust
  - `skills/openpage-test/references/snapshot-refs.md`
    - now explains `_boundary.origin` and `origin=...` in wrapped snapshot text
- Interpretation:
  - this keeps active guidance aligned with the current repo state without churning archived historical material
  - it remains fully outside browser/CDP/element truth sources


## Local truth refresh (2026-05-30, origin-propagation pass)
- This pass extended the already-borrowed trust-boundary/output design from `snapshot` into more read surfaces in `rust/src/cli/serve.rs`.
- Before this pass:
  - `snapshot` already returned `origin`
  - boundary metadata could include `origin` when the result object had it
  - but many other page-read RPC results still did not carry `origin`, so the boundary design did not help there
- Landed code changes in `rust/src/cli/serve.rs`:
  - `webpage.html` now returns `origin` and best-effort `title` alongside `html`
  - `webpage.run_js` / `page.run_js` now returns `origin` alongside `value`
  - `page.selected_text` now returns `origin` alongside `text`
  - `element.text` now returns `origin` alongside `text`
  - `element.html` now returns `origin` alongside `html`
  - `element.attr` now returns `origin` alongside `value`
  - snapshot internals were lightly deduplicated through:
    - `current_page_origin(...)`
    - `current_page_title(...)`
- Verification:
  - `cargo fmt --manifest-path rust/Cargo.toml -- rust/src/cli/serve.rs`
  - `cargo check --manifest-path rust/Cargo.toml`
  - real CLI smoke with temporary `OPENPAGE_HOME` and explicit browser path:
    - `browser start about:blank --session origin-smoke --replace --headless --browser-path /Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
    - `js "document.body.innerHTML = ..." --session origin-smoke`
    - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=500 openpage html --session origin-smoke`
    - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=500 openpage text '#hero' --session origin-smoke`
    - `browser stop --session origin-smoke`
- Observed runtime proof:
  - `js` result now includes `origin: "about:blank"`
  - `html` result now includes:
    - `origin: "about:blank"`
    - `_boundary.origin: "about:blank"`
    - wrapped marker `origin=about:blank`
  - `text` result now includes:
    - `origin: "about:blank"`
    - `_boundary.origin: "about:blank"`
    - wrapped marker `origin=about:blank`
- Interpretation:
  - this is a direct AI-facing outer-shell improvement
  - it broadens the value of boundary/origin design beyond snapshot alone
  - it still does not touch browser/CDP/element truth sources


## Local truth refresh (2026-05-30, active-surface grep + payload-helper test pass)
- Re-audited the active CLI/user surface after the latest outer-shell changes.
- Current grep result:
  - no live `serve --stdio` surface found in `rust/src/cli`, `README.md`, or `skills/openpage-test/*`
  - no live CLI-side `open_page()` / `load_session()` / `save_session()` / `Browser::connect()` execution path found in `rust/src/cli`
  - no live old `page get / page url / page title / page screenshot` user surface found outside archived/tracking docs
- Landed code changes in `rust/src/cli/serve.rs`:
  - deduplicated origin/title payload construction into pure helpers:
    - `payload_with_origin(...)`
    - `payload_with_origin_and_title(...)`
    - `payload_object(...)`
  - reused those helpers for:
    - `webpage.html`
    - `webpage.run_js` / `page.run_js`
    - `page.selected_text`
    - `element.text`
    - `element.html`
    - `element.attr`
    - snapshot root payload
- Why this pass matters:
  - it keeps the AI-facing trust-boundary/origin design centralized in the CLI/daemon shell
  - it adds regression proof without touching browser/CDP/element truth sources
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml payload_with_origin -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_includes_origin_in_boundary_metadata -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_reuses_ready_matching_daemon -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_kills_ready_version_mismatch -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_kills_alive_unready_daemon_after_grace -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml format_snapshot_text_includes_title_origin_refs_and_attrs -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml snapshot_refs_builds_ref_index -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
- Current machine truth from this pass:
  - `browser list` returned 5 healthy sessions:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `human-flow`
    - `smoke-history2`
    - `smoke-shot`
  - `doctor --quick` returned:
    - `pass=8`
    - `warn=1`
    - `fail=1`
    - `info=2`
    - `fixable=3`
    - `total=12`
  - the remaining fail is still `browser.executable`
  - the remaining warn is still legacy one-shot session JSON residue under `~/.openpage/sessions`
- Interpretation:
  - current evidence still supports the claim that TCP daemon is the only active CLI execution truth
  - remaining “old path” evidence is now mostly archival/tracking material, not active code or active user docs


## Local truth refresh (2026-05-30, doctor summary id pass)
- This pass stayed entirely in the CLI/daemon shell.
- Motivation:
  - `doctor --quick` already exposed counts in `summary`
  - but scripts/agents still had to rescan every `checks[]` entry to answer:
    - which checks are warnings right now?
    - which checks are outright failures?
    - which checks have a fix suggestion?
- Landed code changes in `rust/src/cli/doctor.rs`:
  - `Summary` now also returns:
    - `warn_ids`
    - `fail_ids`
    - `info_ids`
    - `fixable_ids`
  - `summarize(...)` now populates those lists in check order
- Why this is aligned:
  - it is a direct non-CDP outer-shell improvement
  - it helps future agents/scripts reason about the current local machine state without reimplementing filtering logic
  - it does not touch browser launch internals, CDP transport, element lookup, or interaction truth sources
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - current runtime summary now includes:
      - `warn_ids=["env.legacy_sessions"]`
      - `fail_ids=["browser.executable"]`
      - `info_ids=["browser.executable.hint","browser.launch"]`
      - `fixable_ids=["env.legacy_sessions","browser.executable","browser.launch"]`
- Active-doc sync done in the same pass:
  - `skills/openpage-test/references/cli-smoke.md`
    - updated current local session count from 4 to 5 healthy sessions
    - now mentions the actionable summary id lists
  - `README.md`
    - doctor section now mentions actionable summary id lists
- Interpretation:
  - this makes `doctor` more useful as the canonical local-state audit entrypoint while keeping the implementation fully outside browser/CDP/element truth sources


## Local truth refresh (2026-05-30, deprecated-cli rejection guard pass)
- This pass focused on making the “unique active protocol surface” claim testable, not just documented.
- Motivation:
  - active-surface grep already showed no live `serve --stdio` or old `page *` user surface
  - but grep alone is soft evidence; a later parser change could accidentally reintroduce those legacy entrypoints
- Landed code changes in `rust/src/cli/oneshot.rs` tests:
  - added parser rejection tests for:
    - `serve --stdio`
    - `page get`
    - `page url`
    - `page title`
    - `page screenshot`
- Why this is aligned:
  - it directly protects the “TCP daemon is the only active CLI execution truth” invariant
  - it still does not touch browser/CDP/element truth sources
  - it turns a historical migration claim into executable regression coverage
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml rejects_serve_stdio_flag -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_get_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_url_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_title_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_screenshot_command -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Active-doc sync in the same pass:
  - `README.md`
    - now explicitly states these removed legacy surfaces are intentionally rejected
  - `skills/openpage-test/references/cli-smoke.md`
    - now explicitly states the same and ties it to parser tests
- Interpretation:
  - current evidence is now stronger than grep-only evidence: the removed protocol/command surfaces are both absent from the active user surface and actively guarded by parser tests


## Local truth refresh (2026-05-30, stable runtime error-kind pass)
- This pass continued strictly at the CLI/daemon shell boundary.
- Motivation:
  - runtime JSON failures still used generic `openpage` in several places
  - that forced automation to scrape human-readable message text instead of matching stable machine categories
- Landed code changes:
  - `rust/src/cli/protocol.rs`
    - added `openpage_error_kind(...)`
    - added `simple_openpage_error(...)`
    - added `response_openpage_error(...)`
  - `rust/src/cli/mod.rs`
    - top-level CLI runtime JSON failures now use `simple_openpage_error(...)`
  - `rust/src/cli/serve.rs`
    - raw TCP daemon runtime failures now use `response_openpage_error(...)`
  - `rust/src/cli/oneshot.rs`
    - `batch` command failures now use `simple_openpage_error(...)`
- Stable runtime `error.kind` values now include categories such as:
  - `unsupported_operation`
  - `browser_operation`
  - `timeout`
  - `io`
  - `serialization`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml openpage_error_kind_maps_variants_to_stable_strings -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_uses_stable_kind_and_message -- --nocapture`
    - passed
  - runtime smoke: `printf '[[\"doctor\"]]' | openpage batch`
    - returned `{"error":{"kind":"unsupported_operation",...},"ok":false}`
  - runtime smoke: raw TCP daemon with invalid target
    - request: `{"id":"1","op":"webpage.title","target":"missing","params":{}}`
    - response: `{"id":"1","ok":false,"error":{"kind":"browser_operation",...}}`
- Current local compile-chain note:
  - while landing this pass, the current worktree exposed a small compile blocker in `rust/src/webpage.rs`
  - it was not a browser/CDP semantic issue; it was a wrapper-level naming drift against current `SessionPage` methods
  - minimal fix applied:
    - `timeout_millis()` -> `timeout_secs()`
    - `set_timeout_millis(...)` -> `set_timeout(...)`
    - restore `HashMap` import
  - after that, `cargo check --manifest-path rust/Cargo.toml` passed again
- Active-doc sync in the same pass:
  - `README.md`
    - now states runtime JSON failures expose stable `error.kind`
  - `skills/openpage-test/references/cli-smoke.md`
    - now advises automation to prefer `error.kind` over scraping the human message text
- Interpretation:
  - this makes the active TCP-only CLI/daemon surface more predictable for agents and scripts
  - it still does not touch browser/CDP/element truth sources


## Local truth refresh (2026-05-30, doctor fix pass)
- This pass stayed in the same outer-shell boundary:
  - `rust/src/cli/args.rs`
  - `rust/src/cli/doctor.rs`
  - active repo-local docs under `README.md` and `skills/openpage-test/*`
- Motivation:
  - local audit still showed obsolete `OPENPAGE_HOME/sessions/*.json` residue from the removed one-shot CLI path
  - those files no longer drove the active TCP daemon path, but they kept `doctor --quick` noisy on this machine
- Landed code changes:
  - `rust/src/cli/args.rs`
    - added `doctor --fix`
  - `rust/src/cli/doctor.rs`
    - added `apply_fixes()`
    - added `remove_legacy_session_files()`
    - `doctor` JSON output now includes a `fixed` array
    - added unit test `remove_legacy_session_files_deletes_only_json_entries`
  - active docs updated:
    - `README.md`
    - `skills/openpage-test/SKILL.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `skills/openpage-test/references/session-management.md`
    - `skills/openpage-test/references/install.md`
- Runtime verification:
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml openpage_error_kind_maps_variants_to_stable_strings -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_uses_stable_kind_and_message -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml remove_legacy_session_files_deletes_only_json_entries -- --nocapture`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick --fix`
    - removed:
      - `/Users/yuuu/.openpage/sessions/cli-more-states-2.json`
      - `/Users/yuuu/.openpage/sessions/cli-state-queries.json`
      - `/Users/yuuu/.openpage/sessions/default.json`
      - `/Users/yuuu/.openpage/sessions/test.json`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - now reports `env.legacy_sessions = pass`
    - remaining fail is still `browser.executable`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - currently returns 6 healthy sessions
    - `incomplete=[]`
    - `cleaned=[]`
- Current local machine truth after this pass:
  - `OPENPAGE_HOME=/Users/yuuu/.openpage`
  - `~/.openpage/sessions/` is now empty
  - `~/.openpage/daemon/` currently has 6 healthy sessions:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `definitely-missing`
    - `human-flow`
    - `smoke-history2`
    - `smoke-shot`
  - the only remaining red item in `doctor --quick` is:
    - `browser.executable`
    - repo config currently says `browser_path=chrome`
    - local viable candidate is `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
- Interpretation:
  - legacy protocol residue on this machine is now reduced further without touching browser/CDP/element truth sources
  - the active TCP daemon path remains healthy
  - the next real problem is not protocol uniqueness; it is local browser-path resolution


## Local truth refresh (2026-05-30, browser-path env override pass)
- This pass stayed in the outer-shell/config layer again.
- Motivation:
  - protocol uniqueness was no longer the red item
  - the remaining local blocker was `rust/configs.ini` saying `browser_path=chrome` while this macOS machine only had the app bundle path
  - we did not want to hardcode `/Applications/...` into repo defaults
- Landed code changes:
  - `rust/src/browser.rs`
    - added `OPENPAGE_BROWSER_PATH_ENV`
    - added `browser_path_env_override()`
    - `Browser::launch(...)` now lets `OPENPAGE_BROWSER_PATH` override the configured browser path for the current process
    - added unit test `browser_path_env_override_reads_non_empty_value`
  - `rust/src/cli/doctor.rs`
    - doctor now also reflects that same env override in the effective `browser.config` check text
  - active docs updated:
    - `README.md`
    - `skills/openpage-test/references/install.md`
    - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_path_env_override_reads_non_empty_value -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `OPENPAGE_BROWSER_PATH=\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - passed
  - `OPENPAGE_BROWSER_PATH=\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor`
    - passed
    - included successful live headless launch smoke
  - `OPENPAGE_BROWSER_PATH=\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" OPENPAGE_HOME=/tmp/openpage-env-browser cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session env-browser --replace --headless https://example.com`
    - passed
  - same env override + `title --session env-browser`
    - returned `Example Domain`
  - same env override + `browser stop --session env-browser`
    - passed
- Current local machine truth after this pass:
  - repo default `rust/configs.ini` still says `browser_path=chrome`
  - that stays unchanged
  - machine-local fix is now:
    - `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
  - with that env override active:
    - doctor passes
    - live headless launch passes
    - named-session CLI launch passes
- Interpretation:
  - this is the right outer-shell compromise
  - it avoids polluting repo defaults with a machine-specific path
  - it keeps protocol/CDP/element truth sources untouched


## Local truth refresh (2026-05-30, browser-path guidance sync pass)
- This was a smaller follow-up pass on top of the env override work.
- Motivation:
  - runtime already honored `OPENPAGE_BROWSER_PATH`
  - doctor check output already reflected it
  - but failure/fix text still biased too heavily toward editing `rust/configs.ini`
- Landed changes:
  - `rust/src/cli/doctor.rs`
    - `missing_browser_message(...)` now mentions `OPENPAGE_BROWSER_PATH`
    - `browser_executable_fix(...)` now mentions `OPENPAGE_BROWSER_PATH`
    - existing unit tests updated to assert that the env override path appears in the guidance
  - `skills/openpage-test/references/cli-smoke.md`
    - common failure meanings now recommend `OPENPAGE_BROWSER_PATH` before forcing a repo config edit
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml missing_browser_message_includes_hint_when_present -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml browser_executable_fix_uses_hint_when_present -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml browser_path_env_override_reads_non_empty_value -- --nocapture`
    - passed
  - `OPENPAGE_BROWSER_PATH=\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - still passed after the guidance update
- Interpretation:
  - the active guidance is now consistent with the active runtime behavior
  - the outer-shell/browser-path story is cleaner for future sessions and agents


## Local truth refresh (2026-05-30, runtime config alignment pass)
- This pass tightened the remaining mismatch between doctor and runtime launch behavior.
- Motivation:
  - doctor already derived browser-path truth from `LaunchOptions::from_ini(None)` plus env override
  - raw TCP `webpage.create` still started from bare `LaunchOptions::default()`
  - that meant doctor and runtime could disagree about which browser executable/config path was actually in play
- Landed changes:
  - `rust/src/cli/serve.rs`
    - `webpage.create` now starts from `LaunchOptions::from_ini(None)?`
    - request parameters only override fields when actually present
    - this preserves config defaults instead of wiping them with `None`
  - docs updated:
    - `README.md`
    - `skills/openpage-test/references/install.md`
    - `skills/openpage-test/references/cli-smoke.md`
- Verification plan/evidence:
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `OPENPAGE_BROWSER_PATH=\"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" OPENPAGE_HOME=/tmp/openpage-runtime-align cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session align --replace --headless https://example.com`
    - passed
  - same env override + `title --session align`
    - returned `Example Domain`
  - same env override + `browser stop --session align`
    - passed
  - without env override:
    - `OPENPAGE_HOME=/tmp/openpage-runtime-align-noenv cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session align-noenv --replace --headless https://example.com`
    - failed with `error.kind=\"browser_operation\"`
    - message showed underlying browser launch failure caused by the unresolved configured executable
- Interpretation:
  - this reduces one more source of “two truths” in the outer shell
  - it does not touch CDP, DOM, element lookup, or interaction internals


## Local truth refresh (2026-05-30, session config alignment pass)
- This pass continued the same outer-shell convergence work, but on the session side of `webpage.create`.
- Motivation:
  - runtime launch already started from `LaunchOptions::from_ini(None)`
  - but runtime session creation still started from `SessionOptions::default()`
  - that left launch config and session config using different truth chains inside the same daemon entrypoint
- Current local machine truth rechecked during this pass:
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - returned 6 healthy sessions
    - 0 incomplete
    - 0 cleaned
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - failed only at `browser.executable`
    - unlike earlier passes, the configured executable in the current dirty worktree is now `/tmp/dp-browser`
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - passed
- Landed changes:
  - `rust/src/cli/serve.rs`
    - added `session_options_from_request(...)`
    - `webpage.create` now starts session config from `SessionOptions::from_ini(None)?`
    - request params only override `timeout_secs` / `user_agent` when explicitly present
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml session_options_from_request_ -- --nocapture`
    - passed
    - confirmed ini defaults are preserved when request params omit those fields
    - confirmed explicit request params still override ini values
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session session-config-noenv --headless https://example.com`
    - failed with `error.kind="browser_operation"`
    - underlying cause remained unresolved configured executable
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session session-config-seq --headless https://example.com`
    - passed
  - same env override + `title --session session-config-seq`
    - returned `Example Domain`
  - same env override + `browser stop --session session-config-seq`
    - passed
- Interpretation:
  - runtime now starts both launch options and session options from the same ini/config truth
  - this is still strictly outer-shell work
  - it does not borrow competitor CDP, snapshot internals, element lookup, or interaction logic


## Local truth refresh (2026-05-30, request retry re-ensure pass)
- This pass focused on another outer-shell stability gap in the unique TCP daemon path.
- Motivation:
  - `send_request()` previously called `ensure_daemon()` only once, before entering the retry loop
  - if the daemon died after that first ensure but before the real socket round-trip completed, later retries would only resend against the bad state instead of re-running sidecar/restart logic
- Landed changes:
  - `rust/src/cli/connection.rs`
    - extracted `send_request_with_retry(...)`
    - `send_request()` now re-runs ensure logic before every retry attempt
    - transient request recovery now reuses the same stale-daemon cleanup path as initial daemon startup
  - `skills/openpage-test/references/cli-smoke.md`
    - no longer hard-codes a specific healthy-session count
    - now records that browser-list counts are runtime-local and drift as named-session smoke daemons accumulate
    - current dirty-worktree browser-path truth on this machine updated from old `chrome` wording to `/tmp/dp-browser`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_ -- --nocapture`
    - passed
    - verified re-ensure happens again after a transient error
    - verified non-transient errors still stop immediately
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - returned only healthy sessions and no incomplete / cleaned sidecars
    - exact count drifted again during smoke because browser stop does not necessarily remove daemon sessions
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session retry-shell-check --headless https://example.com`
    - passed
  - same env override + `title --session retry-shell-check`
    - returned `Example Domain`
  - same env override + `browser stop --session retry-shell-check`
    - passed
- Verification chain blockers encountered:
  - current worktree had unrelated compile blockers in:
    - `rust/src/settings.rs` (untracked new file)
    - `rust/src/page.rs` test patterns
  - minimal local fixes were applied only to restore `cargo check` / targeted `cargo test` evidence
  - these are not part of the outer-shell design change itself
- Interpretation:
  - the TCP daemon path is now more resilient to daemon death between initial ensure and request send
  - this is a direct borrow of competitor outer-shell thinking, not of competitor CDP or element internals


## Local truth refresh (2026-05-30, browser stop lifecycle pass)
- This pass focused on shutdown semantics for named-session CLI flows.
- Motivation:
  - `browser stop` previously tried `webpage.quit`, then best-effort `daemon.shutdown`, then blindly removed sidecars
  - that left a gap where an alive but unresponsive daemon could survive behind a success-shaped CLI result
- Landed changes:
  - `rust/src/cli/connection.rs`
    - added `shutdown_daemon(...)`
    - graceful path: send `daemon.shutdown` directly when the daemon is ready
    - verification path: poll until the daemon is actually gone
    - fallback path: if still alive, force stale kill and cleanup
    - returned structured result with `had_daemon` / `forced`
  - `rust/src/cli/oneshot.rs`
    - `browser stop` now uses `shutdown_daemon(...)`
    - JSON result now includes `had_daemon` and `forced`
  - `skills/openpage-test/references/session-management.md`
    - active docs now explain that browser stop first tries graceful shutdown and falls back to forced cleanup when needed
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml cli::connection::tests:: -- --nocapture`
    - passed
    - includes `shutdown_daemon_cleans_stale_sidecars_when_process_is_gone`
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start --session stop-shell-check --headless https://example.com`
    - passed
  - same machine + `browser stop --session stop-shell-check`
    - returned `{ "stopped": true, "had_daemon": true, "forced": false, ... }`
  - same machine + `browser list`
    - no longer contained `stop-shell-check`
- Interpretation:
  - stop semantics now match the same “graceful first, force if necessary” outer-shell philosophy already used in stale-daemon startup recovery
  - this is still non-CDP shell work only

## Local truth refresh (2026-05-30, compat-only audit + local-state recheck)
- This pass was a repo-local truth refresh, not a new protocol change.
- Motivation:
  - verify the current dirty worktree still has one active TCP execution path
  - verify the `dp` helper is now visibly compat-only instead of silently looking like a second surface
  - write the refreshed local-machine facts into the persistent tracking files and borrow-analysis document
- Current local machine truth rechecked during this pass:
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
    - returned 6 healthy sessions
    - 0 incomplete
    - 0 cleaned
    - current healthy sessions were:
      - `cli-more-states-2`
      - `cli-state-queries`
      - `definitely-missing`
      - `human-flow`
      - `smoke-history2`
      - `smoke-shot`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - failed only at `browser.executable`
    - the failing configured executable in the current dirty worktree is `/tmp/dp-browser`
  - `OPENPAGE_BROWSER_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - passed
  - same env override + `browser start --session latest-local-audit --headless https://example.com`
    - passed
  - same env override + `title --session latest-local-audit`
    - returned `Example Domain`
  - same env override + `browser stop --session latest-local-audit`
    - returned `forced=false` and `had_daemon=true`
  - `browser list` after that stop
    - no longer contained `latest-local-audit`
- Additional protocol-surface verification:
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_get_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_url_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_title_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_legacy_page_screenshot_command -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rejects_serve_stdio_flag -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - all passed
- Interpretation:
  - the active CLI truth is still a single TCP daemon execution path
  - `dp` is now guarded as compat-only in both help text and tests, so it should not be treated as a second protocol branch
  - the suspicious session name `definitely-missing` is not evidence of broken state by itself; current runtime checks say it is alive and ready
  - the next safe borrow target remains outer-shell only: connection / doctor / protocol / agent docs
  - do not borrow competitor `cli/src/native/*`, CDP transport, locator logic, or interaction internals

## Local truth refresh (2026-05-30, dp entry tightening pass)
- This pass was a small active-surface cleanup after the local-state audit.
- Motivation:
  - the repo already described `dp` as compat-only
  - but `rust/src/cli/mod.rs` still allowed the `openpage` binary to silently enter compat mode when root flags like `--set-browser-path` were passed
  - that kept a second user-facing entry shape alive even though the protocol truth was supposed to be unique
- Landed change:
  - `rust/src/cli/mod.rs`
    - `should_use_dp_compat_mode(...)` now returns true only when the executable stem is `dp`
    - the active `openpage` binary no longer accepts `dp` compat flags as a hidden fallback path
  - updated unit test:
    - `detects_dp_compat_mode_only_for_dp_binary`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml detects_dp_compat_mode_only_for_dp_binary -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --set-browser-path /tmp/chrome`
    - now fails with `unexpected argument '--set-browser-path' found`
- Interpretation:
  - this is a small but real protocol-surface cleanup
  - `dp` remains available as a compat helper binary
  - `openpage` no longer has a hidden root-flag side door into compat behavior
  - this stays within the outer shell and does not touch browser/CDP/element internals

## Local truth refresh (2026-05-30, top-level parse JSON shell pass)
- This pass continued the same outer-shell borrowing direction, but at the CLI parser boundary.
- Motivation:
  - runtime failures already returned stable JSON with `error.kind`
  - but clap parse/input failures still escaped as raw human-oriented text
  - that meant removed legacy surfaces like `page url` and rejected compat flags like `--set-browser-path` did not use the same machine-facing shell as the active TCP CLI
- Landed changes:
  - `rust/src/cli/mod.rs`
    - added `clap_error_payload(...)`
    - added `print_clap_error(...)`
    - `run_from_args(...)` and `run_dp_compat_from_args(...)` now route non-help/version clap failures through:
      - `{"ok":false,"error":{"kind":"invalid_input","message":"..."}}`
    - help/version still keep the normal clap text output
  - active docs updated:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `rustfmt rust/src/cli/mod.rs`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml parse_errors_render_machine_friendly_json_shell -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml help_output_keeps_text_shell -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml detects_dp_compat_mode_only_for_dp_binary -- --nocapture`
    - passed
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
    - passed
  - `cargo check --manifest-path rust/Cargo.toml`
    - passed
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
    - now returns JSON with `error.kind="invalid_input"`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --set-browser-path /tmp/chrome`
    - now returns JSON with `error.kind="invalid_input"`
- Interpretation:
  - this is still outer-shell only
  - removed legacy CLI surfaces no longer escape as a different error medium
  - the active machine interface is more uniform without borrowing any competitor browser, CDP, snapshot, element, or interaction internals

## Local truth refresh (2026-05-30, protocol/doc sync recheck)

- Intent:
  - recheck the current machine-local runtime truth before touching more outer-shell work
  - patch active docs/tracking files so they stop repeating stale `browser_path=chrome` wording
  - record the current protocol-shell truth for raw TCP clients, not just the named-session CLI
- Commands run:
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `sed -n '1,220p' rust/configs.ini`
  - `ls -la ~/.openpage/daemon`
  - `ls -la ~/.openpage/sessions`
- Observed runtime truth:
  - `rust/configs.ini` currently resolves to `browser_path=/tmp/dp-browser`
  - `browser list` returned 6 healthy sessions, 0 incomplete, 0 cleaned:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `definitely-missing`
    - `human-flow`
    - `smoke-history2`
    - `smoke-shot`
  - `doctor --quick` returned exactly 1 fail:
    - `browser.executable`
    - the failing configured executable is `/tmp/dp-browser`
  - with `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, `doctor --quick` returned all pass except the expected `browser.launch` info skip
  - `~/.openpage/sessions` still exists but is empty, so `env.legacy_sessions` is currently `pass`, not a warn/fail item
  - `~/.openpage/daemon` still contains both active sidecars and many historical log files, so inventory/doctor remain necessary even though the active sessions are healthy
- Doc/tracking cleanup made from that evidence:
  - `README.md`
    - stable `error.kind` list now explicitly includes raw TCP daemon `invalid_json` and `tcp_error`
  - `skills/openpage-test/references/cli-smoke.md`
    - common-failure text no longer hardcodes the old `chrome` wording
    - now records the current dirty-worktree failure target `/tmp/dp-browser`
    - now explains raw TCP `invalid_json` and `tcp_error`
  - `task_plan.md`
    - current-state bullets now use `/tmp/dp-browser` instead of stale `chrome` wording where they were describing present truth
    - status now records that this turn rechecked `browser list`, plain `doctor --quick`, and override `doctor --quick`
- Interpretation:
  - the active protocol shell is still single-path TCP
  - the remaining red item on this machine is still browser executable resolution, not protocol drift
  - the active docs now better match the current dirty worktree and the current daemon error surface

## Local truth refresh (2026-05-30, latest protocol audit recheck)

- Intent:
  - re-verify the current local protocol truth before borrowing more competitor outer-shell pieces
  - record the latest machine-local daemon inventory so the tracking docs stop lagging the runtime
  - confirm whether old protocol wording is still live code, intentionally rejected documentation, or archived residue
- Commands run:
  - `git status --short`
  - `rg -n "serve --stdio|page get|page url|page title|page screenshot|open_page\\(|load_session\\(|save_session\\(|Browser::connect\\(" rust README.md skills rust_progress_report.md 协议迁移审计-v1.md -g '!**/target/**'`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed runtime truth:
  - the repo is still very dirty; this remains a path-focused task and is not ready for a broad git checkpoint
  - `browser list` now returns 7 healthy sessions, 0 incomplete, 0 cleaned:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `definitely-missing`
    - `human-flow`
    - `smoke-history2`
    - `smoke-shot`
    - `smoke_eval_5554`
  - `definitely-missing` is still healthy despite the suspicious name; current evidence says `alive=true` and `ready=true`
  - `doctor --quick` still returns exactly 1 fail:
    - `browser.executable`
    - the failing configured executable is still `/tmp/dp-browser`
  - `cargo check` passed on the current dirty worktree
- Active-surface grep interpretation:
  - live code hits for old protocol wording are still limited to the reject tests in `rust/src/cli/oneshot.rs`
  - active user-facing doc hits remain intentional removed-surface notes in:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
  - the bulk of remaining `serve --stdio` / `page *` / one-shot attach wording still lives in:
    - `rust_progress_report.md`
    - `协议迁移审计-v1.md`
    - tracking files like `task_plan.md`, `notes.md`, `claude-progress.txt`
- Interpretation:
  - the unique active CLI/daemon protocol is still TCP-only
  - there is still no evidence of the old execution path becoming active again
  - the next useful work remains outer-shell cleanup and selective borrowing, not browser/CDP/element rework

## Local truth refresh (2026-05-30, compiled help surface + skill doc sync)

- Intent:
  - verify the compiled help text still encodes the same single-protocol truth as the implementation
  - update active agent-facing smoke docs with the latest local runtime evidence instead of stale session counts
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
- Observed truth:
  - `cargo check` passed on the current dirty worktree
  - compiled `openpage --help` now explicitly says:
    - the active CLI protocol is TCP-backed daemon only
    - there is no separate stdio daemon mode
    - there is no direct browser-execution CLI path
  - compiled `openpage serve --help` now explicitly says:
    - TCP-only daemon mode
    - `--port 0` requests an OS-assigned port
    - the removed `serve --stdio` surface stays rejected
  - `browser list` still returned 7 healthy sessions, 0 incomplete, 0 cleaned:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `definitely-missing`
    - `human-flow`
    - `smoke-history2`
    - `smoke-shot`
    - `smoke_eval_5554`
  - `doctor --quick` still failed only at `browser.executable=/tmp/dp-browser`
- Doc cleanup made from that evidence:
  - `README.md`
    - now says the compiled help text itself is part of the protocol guardrail
  - `skills/openpage-test/references/cli-smoke.md`
    - now includes `smoke_eval_5554` in the latest local runtime inventory
    - now records that compiled `openpage --help` / `serve --help` carry the TCP-only truth directly
  - `竞品文档-考虑借鉴的部分v1.md`
    - now records CLI help as another active guardrail worth preserving when borrowing outer-shell design
- Interpretation:
  - single-protocol truth is now aligned across implementation, parser tests, compiled help text, README, and active smoke docs
  - the remaining live issue on this machine is still browser executable resolution, not protocol ambiguity

## Active-session boundary sync (2026-05-30)

- Intent:
  - sync the latest shell-level session bootstrap rule into active docs/help so later work does not accidentally revive the old silent auto-start behavior
  - keep the borrowing boundary explicit: shell/session lifecycle only, not browser/CDP internals
- Current implementation truth:
  - `rust/src/cli/oneshot.rs::rpc_webpage()` now routes through `send_request_existing(...)`
  - `rust/src/cli/oneshot.rs::run_goto()` still uses `ensure_webpage_session(...)`, so `goto` remains a narrow bootstrap path
  - `rust/src/cli/oneshot.rs::start_browser()` remains the explicit bootstrap path
  - `rust/src/cli/connection.rs::ensure_existing_daemon()` is the guardrail that produces the inactive-session failure
- Files updated from that truth:
  - `rust/src/cli/args.rs`
  - `rust/src/cli/mod.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- Why this matters:
  - it is a direct competitor-inspired outer-shell refinement
  - it keeps TCP daemon lifecycle predictable
  - it does not touch `browser.rs`, `page.rs`, `element.rs`, or any CDP/locator internals

## Local truth refresh (2026-05-30, compile gate recovery + latest runtime inventory)

- Intent:
  - restore a verifiable local baseline before claiming anything about the current TCP-only protocol surface
  - sync the latest machine-local runtime truth back into the persistent files instead of leaving them at the previous 7-session snapshot
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
- Compile blockers fixed first:
  - `rust/src/browser.rs::tab_infos()` needed an explicit `Ok::<Vec<TabInfo>, OpenPageError>(...)`
  - `rust/src/browser.rs::wait_for_new_tab()` had stale call sites after `find_new_tab_id(..., explicit_current_tab: bool)` changed shape
  - this was a minimal compile-gate repair only; it did not change CLI/daemon protocol intent
- Observed runtime truth after the repair:
  - `cargo check` passed again on the current dirty worktree
  - `browser list` now returns 8 healthy sessions, 0 incomplete, 0 cleaned:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `definitely-missing`
    - `human-flow`
    - `human-gap-check`
    - `smoke-history2`
    - `smoke-shot`
    - `smoke_eval_5554`
  - `definitely-missing` still must not be deleted by name alone; runtime evidence still says `alive=true` and `ready=true`
  - plain `doctor --quick` still fails only at `browser.executable=/tmp/dp-browser`
  - with `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, `doctor --quick` passes
  - compiled `openpage --help` still says:
    - active CLI protocol = TCP-backed daemon only
    - there is no separate stdio daemon mode
    - there is no direct browser-execution CLI path
  - compiled `openpage serve --help` still says:
    - TCP-only daemon mode
    - `--port 0` uses an OS-assigned port
    - removed `serve --stdio` stays rejected
  - the 3 targeted tests above all passed again
- Interpretation:
  - the active TCP daemon shell is still single-path after the latest local recheck
  - the current machine-local red item is still browser executable resolution, not protocol drift
  - the latest runtime inventory is now 8 healthy sessions, but that remains runtime-local truth rather than a repository invariant

## Local truth refresh (2026-05-31, current worktree re-audit)

- Intent:
  - replace the previous session's stale compile-blocker assumption with current evidence
  - record one concrete shell-only integration pattern that already exists locally and does not require competitor native runtime borrowing
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_permissions_set -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_clipboard_write -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser start https://example.com --session latest-local-audit-20260531 --headless`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- title --session latest-local-audit-20260531`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --session latest-local-audit-20260531`
- Current worktree truth:
  - `cargo check` passes again; the previous `oneshot.rs` non-exhaustive `Clipboard` / `Permissions` blocker is no longer current
  - compiled `openpage --help` still states:
    - active CLI protocol = TCP-backed daemon only
    - no separate stdio daemon mode
    - no direct browser-execution CLI path
  - compiled `openpage serve --help` still states:
    - TCP-only daemon mode
    - removed `serve --stdio` stays rejected
  - `browser list` currently returns 15 healthy sessions, 0 incomplete, 0 cleaned
  - plain `doctor --quick` still fails only at `browser.executable=/tmp/dp-browser`
  - with `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, `doctor --quick` passes
  - the ordered smoke `browser start -> title -> browser stop` passes with the same override, and `title` returns `Example Domain`
- Concrete shell-only integration pattern already present:
  - `rust/src/cli/args.rs` exposes `Clipboard` and `Permissions`
  - `rust/src/cli/oneshot.rs` routes them only through daemon RPC
  - `rust/src/cli/serve.rs` dispatches only to `page.clipboard_*` / `page.set_permission` / `page.reset_permissions`
  - `rust/src/webpage.rs` wraps the capability in driver mode
  - `rust/src/page.rs` owns the real implementation and runtime regression coverage
- Interpretation:
  - this is the right borrowing direction for OpenPage: shell/control-plane design only
  - there is no need to import `agent-browser` native action / CDP / locator code for this class of feature
  - the remaining machine-local red item is still browser executable resolution, not protocol multiplicity

## Local truth refresh (2026-05-31, browser-logs shell pass)

- Intent:
  - verify the newly added `browser logs` shell surface rather than assuming the earlier patch compiled cleanly
  - record the latest machine-local session inventory and doctor summary in the same pass
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_logs_tail -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage browser logs --session human-flow --tail 20`
  - `rust/target/debug/openpage browser logs --session clipboard-probe-20260531 --tail 20`
  - `rust/target/debug/openpage --help`
  - `rust/target/debug/openpage serve --help`
- Observed runtime truth:
  - `cargo check` passed
  - both new `browser logs` tests passed
  - the three existing protocol/compat guard tests passed again
  - `browser list` now returns 17 healthy sessions, 0 incomplete, 0 cleaned:
    - `cli-more-states-2`
    - `cli-state-queries`
    - `clipboard-probe-20260531`
    - `definitely-missing`
    - `human-clipboard-audit-20260531`
    - `human-flow`
    - `human-gap-check`
    - `human-window-audit-20260531`
    - `human-window-audit-20260531-b`
    - `human-window-audit-20260531-c`
    - `human-window-smoke-20260531`
    - `human-window-switch-20260531`
    - `local-audit-20260531-seq`
    - `local-audit-20260531-tcp`
    - `smoke-history2`
    - `smoke-shot`
    - `smoke_eval_5554`
  - plain `doctor --quick` still reports:
    - `pass=21`
    - `warn=0`
    - `fail=1`
    - `info=1`
    - `total=23`
    - `fail_ids=["browser.executable"]`
    - `fixable_ids=["browser.executable","browser.launch"]`
    - current failing configured executable remains `/tmp/dp-browser`
  - override `doctor --quick` still reports:
    - `fail=0`
    - `info=1`
    - `fixable_ids=["browser.launch"]`
  - `browser logs --session human-flow --tail 20` returned:
    - `exists=false`
    - `content=null`
    - meaning that session currently has no persisted stderr log file
  - `browser logs --session clipboard-probe-20260531 --tail 20` returned:
    - `exists=true`
    - tailed content containing `WebSocket protocol error: Connection reset without closing handshake`
  - compiled help still states:
    - active CLI protocol = TCP-backed daemon only
    - removed `serve --stdio` stays rejected
- Interpretation:
  - `browser logs` is a valid next borrow target because it is pure shell/control-plane behavior
  - the new surface is useful for diagnosing daemon sessions without importing any competitor browser runtime code
  - protocol uniqueness is still defended by the existing help + tests; the new command does not create a second execution path

## Local truth refresh (2026-05-31, post-element-compile-recovery recheck)

- Intent:
  - re-verify the current worktree after the latest compile-recovery edit rather than assuming earlier notes are still current
  - sync the latest local runtime truth into tracking docs without widening the implementation scope
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml dp_compat_help_marks_surface_as_compat_only -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_logs_tail -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage browser logs --session human-flow --tail 20`
  - `rust/target/debug/openpage browser logs --session clipboard-probe-20260531 --tail 20`
  - `rust/target/debug/openpage --help`
  - `rust/target/debug/openpage serve --help`
- Current worktree truth:
  - `cargo check` passes
  - the current worktree includes a minimal compile-recovery adjustment in `rust/src/element.rs`:
    - `Frame::new(...)` now receives `Arc::clone(self.none_element_runtime_config_handle())`
    - this is a signature-alignment fix only, not a protocol or borrow-boundary change
  - all protocol/compat guard tests above pass again
  - all three `browser logs` parse/tail tests pass again
  - `browser list` still returns 17 healthy sessions, 0 incomplete, 0 cleaned
  - plain `doctor --quick` still reports:
    - `pass=21`
    - `warn=0`
    - `fail=1`
    - `info=1`
    - `total=23`
    - `fail_ids=["browser.executable"]`
    - `fixable_ids=["browser.executable","browser.launch"]`
    - current failing configured executable remains `/tmp/dp-browser`
  - override `doctor --quick` now explicitly reports:
    - `pass=22`
    - `warn=0`
    - `fail=0`
    - `info=1`
    - `total=23`
    - `fixable_ids=["browser.launch"]`
  - `browser logs --session human-flow --tail 20` still returns `exists=false` and `content=null`
  - `browser logs --session clipboard-probe-20260531 --tail 20` still returns `exists=true` and tailed stderr containing `Connection reset without closing handshake`
  - compiled help still states:
    - active CLI protocol = TCP-backed daemon only
    - removed `serve --stdio` stays rejected
- Interpretation:
  - the active protocol truth is still single-path TCP
  - the latest compile-recovery fix did not reopen any browser/CDP/locator borrowing question
  - the correct next borrow direction remains shell/control-plane only: `connection.rs`, `doctor.rs`, output governance, and agent-facing docs

## Local truth refresh (2026-05-31, doctor-fix verification pass)

- Intent:
  - verify the latest `doctor --quick --fix` borrow point with current worktree evidence instead of treating the earlier handoff as sufficient proof
  - confirm the active CLI still rejects removed protocol surfaces at runtime, not just in tests and docs
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incomplete_unready_daemon_session -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --help`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor`
  - synthetic smoke with a temporary `OPENPAGE_HOME=/tmp/openpage-doctor-fix-*` containing:
    - `sessions/legacy-a.json`
    - `daemon/stale-daemon.pid`
    - `daemon/stale-daemon.port`
    - a live `/bin/sleep 30` process wired to `daemon/incomplete-daemon.pid`
    - `daemon/incomplete-daemon.port`
    - then `openpage doctor --quick --fix`
- Observed truth:
  - `cargo check` passed
  - both new `apply_fixes_*` tests passed
  - `openpage page url` returned top-level JSON with `error.kind="invalid_input"`
  - `openpage serve --stdio` returned top-level JSON with `error.kind="invalid_input"`
  - `browser list` still returns 17 healthy sessions, 0 incomplete, 0 cleaned
  - plain `doctor --quick` still reports:
    - `pass=21`
    - `warn=0`
    - `fail=1`
    - `info=1`
    - `total=23`
    - `fail_ids=["browser.executable"]`
    - `fixable_ids=["browser.executable","browser.launch"]`
    - current failing configured executable remains `/tmp/dp-browser`
  - override `doctor --quick` still reports `fail=0`
  - override full `doctor` now also reports `fail=0` and `pass=23`, including a successful live headless launch smoke
  - synthetic `doctor --quick --fix` returned `fixed[]` entries for:
    - removing `sessions/legacy-a.json`
    - removing stale sidecars for `stale-daemon`
    - stopping and removing incomplete unready session `incomplete-daemon`
  - after that synthetic fix:
    - the spawned `sleep` PID was no longer alive
    - `find "$tmp" -maxdepth 3 -type f` returned no remaining sidecar files
    - a second `doctor --quick` against the same temp home showed no legacy-session or incomplete-sidecar residue
- Interpretation:
  - `doctor --quick --fix` is now proven as a shell-only borrow point, not just a speculative design match
  - the new fix path stays inside sidecars / daemon lifecycle and does not cross into OpenPage browser/CDP/locator internals
  - the active protocol truth remains single-path TCP, and removed surfaces are still rejected at runtime in the same JSON shell

## Local truth refresh (2026-05-31, stop-all shell borrow pass)

- Intent:
  - verify the next non-CDP borrow point with current worktree evidence
  - keep the implementation strictly in CLI shell / daemon inventory / shutdown control flow
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo build --manifest-path rust/Cargo.toml --bin openpage`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser stop --help`
  - `cargo test --manifest-path rust/Cargo.toml parses_browser_stop_all -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_stop_all_sessions_deduplicates_and_keeps_alive_incomplete_sessions -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml parses_batch_with_commands -- --nocapture`
  - synthetic smoke with `OPENPAGE_HOME=/tmp/openpage-stop-all-*`:
    - start two raw daemons via `rust/target/debug/openpage serve --session alpha --port 0` and `... beta ...`
    - `rust/target/debug/openpage browser list`
    - `rust/target/debug/openpage browser stop --all`
    - `rust/target/debug/openpage browser list`
    - `kill -0 <pid>` checks for both daemon pids
  - synthetic single-session regression with `OPENPAGE_HOME=/tmp/openpage-stop-one-*`:
    - `serve --session review --port 0`
    - `browser stop --session review`
    - `browser list`
- Observed truth:
  - `cargo check` passed
  - `cargo build --bin openpage` passed
  - `browser stop --help` now shows:
    - `--session <SESSION>`
    - `--all`
  - `parses_browser_stop_all` passed
  - `browser_stop_all_sessions_deduplicates_and_keeps_alive_incomplete_sessions` passed
  - `parses_batch_with_commands` passed again, so batch strings containing `browser stop` still parse after the stop-args change
  - synthetic stop-all smoke returned:
    - initial `browser list` with healthy `alpha` and `beta`
    - `browser stop --all` result `{"stopped":2,"sessions":["alpha","beta"],"failed":[],"all_stopped":true}`
    - follow-up `browser list` with `sessions=[]`
    - both daemon pids were no longer alive
  - synthetic single-session regression returned:
    - `browser stop --session review` result with `had_daemon=true` and `forced=false`
    - follow-up `browser list` empty
- Interpretation:
  - `browser stop --all` is now verified as another shell-only borrow point
  - the new path stays entirely in daemon inventory + shutdown orchestration
  - no browser/CDP/locator/interaction internals were imported or replaced

## Local truth refresh (2026-05-31, browser-list summary pass)

- Intent:
  - make the current local runtime truth easier for agents/scripts to consume without changing protocol uniqueness or browser internals
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_summary_counts_all_categories -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - synthetic smoke with `OPENPAGE_HOME=/tmp/openpage-list-summary-*`:
    - `rust/target/debug/openpage serve --session summary-check --port 0`
    - `rust/target/debug/openpage browser list`
- Observed truth:
  - `cargo check` passed
  - `browser_inventory_summary_counts_all_categories` passed
  - current-machine `browser list` now reports:
    - `summary.healthy=17`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=17`
  - synthetic single-session smoke reports:
    - `summary.healthy=1`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=1`
- Interpretation:
  - `browser list` now gives a stable machine-friendly summary layer on top of the existing inventory truth
  - this is a shell/output enhancement only; it does not touch browser/CDP/locator internals

## Local truth refresh (2026-05-31, browser-status state pass)

- Intent:
  - expose a cleaner session-state truth for agents/scripts without changing any browser/CDP/locator internals
- Commands run:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo test --manifest-path rust/Cargo.toml incomplete_session_reasons_report_missing_version_and_not_ready -- --nocapture`
  - `cargo build --manifest-path rust/Cargo.toml --bin openpage`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser status --help`
  - synthetic smoke with `OPENPAGE_HOME=/tmp/openpage-status-shapes-*`:
    - `rust/target/debug/openpage serve --session healthy --port 0`
    - one live incomplete session via `/bin/sleep 30` + `.pid/.port` and no `.version`
    - `rust/target/debug/openpage browser status --session healthy`
    - `rust/target/debug/openpage browser status --session incomplete`
    - `rust/target/debug/openpage browser status --session missing`
- Observed truth:
  - `cargo check` passed
  - `incomplete_session_reasons_report_missing_version_and_not_ready` passed
  - synthetic healthy status returned `state="healthy"`
  - synthetic incomplete status returned:
    - `state="incomplete"`
    - `reasons=["missing_version","daemon_not_ready"]`
    - raw `incomplete` sidecar booleans
  - synthetic missing status returned `state="inactive"`
- Interpretation:
  - `browser status` now exposes a stable machine-friendly state layer on top of daemon lifecycle truth
  - this is still a shell/control-plane enhancement only

## Local truth refresh (2026-05-31, browser-list entry-state pass)

- Intent:
  - make the current local daemon inventory easier to consume entry-by-entry, not just via top-level counts
  - keep the change strictly in shell/output shaping
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_ -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml incomplete_session_reasons_ -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser list`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - both inventory tests passed:
    - `browser_inventory_summary_counts_all_categories`
    - `browser_inventory_payload_includes_state_and_reasons`
  - `incomplete_session_reasons_report_missing_version_and_not_ready` passed again
  - while re-running the deprecated-surface guard, the current worktree exposed a shell-layer compile blocker:
    - `DownloadsCommand::Open(_)` / `Reveal(_)` existed in `args.rs`
    - `run_downloads(...)` had not covered those variants
  - that blocker is now minimally repaired in `rust/src/cli/oneshot.rs`:
    - `downloads open` resolves a tracked download path and opens it with the OS default app
    - `downloads reveal` resolves a tracked download path and reveals its parent location through the OS shell
  - `download_final_path_requires_non_empty_path` now passes
  - current-machine `browser list` now reports every healthy `sessions[]` entry with `state="healthy"`
  - current-machine runtime remains:
    - `summary.healthy=17`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=17`
  - `openpage_help_marks_tcp_daemon_as_only_active_protocol` passes again
  - `openpage page url` still returns `error.kind="invalid_input"`
  - `serve --stdio` still returns the unified JSON error shell with `error.kind="invalid_input"`
  - user-side git activity is still live:
    - `ps -axo pid,ppid,command | rg 'git add -p|index.lock'`
    - still shows `git add -p rust/src/cli/oneshot.rs` as pid `22000`
- Interpretation:
  - this is another shell-only borrow point from the competitor's session-inventory ergonomics
  - it improves local-state observability without touching browser/CDP/locator/interaction internals
  - the active protocol surface remains uniquely TCP-backed

## Local truth refresh (2026-05-31, browser-logs state-alignment pass)

- Intent:
  - align the shell-level diagnostics surfaces so `browser logs` and `browser status` speak the same state taxonomy
  - keep the change strictly in CLI payload shaping and log reading
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_keeps_last_lines -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_log_tail_handles_zero_and_large_limits -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser logs --session human-flow --tail 5`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - `browser_logs_payload_preserves_state_and_reasons` passed
  - both existing log-tail tests passed again
  - `browser logs --session human-flow --tail 5` now returns:
    - `state="healthy"`
    - `exists=false`
    - `content=null`
    - `path="/Users/yuuu/.openpage/daemon/human-flow.log"`
  - that means the current `human-flow` daemon is healthy, but there is no persisted stderr log file for it yet
  - `openpage_help_marks_tcp_daemon_as_only_active_protocol` passed again
  - both removed surfaces still reject at runtime with the unified JSON shell:
    - `openpage page url` → `error.kind="invalid_input"`
    - `openpage serve --stdio` → `error.kind="invalid_input"`
- Interpretation:
  - this is another shell-only borrow point from the competitor's diagnostics ergonomics
  - it keeps all session-debugging surfaces on the same state taxonomy without touching browser/CDP/locator internals

## Local truth refresh (2026-05-31, doctor-inventory pass)

- Intent:
  - make `doctor --quick` itself expose the current daemon runtime truth, not just check-oriented findings
  - keep the change strictly in doctor payload shaping on top of the existing inventory scan
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - `doctor_inventory_payload_includes_state_and_reasons` passed
  - `summarize_counts_info_fixable_and_total` still passed after the doctor payload change
  - `doctor --quick` now returns a top-level `inventory` block
  - on the current machine that block reports:
    - `summary.healthy=17`
    - `summary.incomplete=0`
    - `summary.cleaned=0`
    - `summary.total=17`
  - healthy `inventory.sessions[]` entries now also carry `state="healthy"`
  - in the same runtime payload, the only fail still remains:
    - `browser.executable`
    - configured path `/tmp/dp-browser`
  - `openpage_help_marks_tcp_daemon_as_only_active_protocol` passed again
  - both removed surfaces still reject with the unified JSON shell:
    - `openpage page url` → `error.kind="invalid_input"`
    - `openpage serve --stdio` → `error.kind="invalid_input"`
- Interpretation:
  - this is another shell-only borrow point from the competitor's diagnostics ergonomics
  - it shortens the path from “run doctor” to “see the actual local daemon truth” without touching browser/CDP/locator internals

## Local truth refresh (2026-05-31, shared reason-taxonomy pass)

- Intent:
  - stop letting `browser list/status/logs` and `doctor` each maintain their own copy of incomplete-session `reasons[]`
  - move that truth to the sidecar/inventory layer itself
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml incomplete_daemon_reasons_report_missing_version_and_not_ready -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - `connection.rs` now owns:
    - shared incomplete-session `reasons[]`
    - shared inventory `summary`
    - shared inventory payload JSON shaping
  - all four targeted tests passed
  - current runtime still agrees across surfaces:
    - `browser list` summary → `healthy=17 / incomplete=0 / cleaned=0 / total=17`
    - `doctor --quick` inventory summary → `healthy=17 / incomplete=0 / cleaned=0 / total=17`
  - `doctor --quick` still has exactly one fail id:
    - `browser.executable`
  - removed surfaces still reject with the unified JSON shell:
    - `openpage page url` → `error.kind="invalid_input"`
    - `openpage serve --stdio` → `error.kind="invalid_input"`
  - user-side git activity is still live:
    - `git add -p rust/src/cli/oneshot.rs`
    - pid `22000`
- Interpretation:
  - this is a control-plane truth-source consolidation, not a browser-kernel change
  - it reduces drift risk for future agent sessions while keeping the TCP-only execution model intact

## Local truth refresh (2026-05-31, browser-log content boundary pass)

- Intent:
  - extend the existing agent-friendly output boundary / truncate chain to daemon log text too
  - keep the change strictly in CLI JSON output shaping
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_wraps_content_field_with_boundaries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml format_output_json_truncates_content_field -- --nocapture`
  - `OPENPAGE_CONTENT_BOUNDARIES=1 OPENPAGE_MAX_OUTPUT_CHARS=40 cargo run --manifest-path rust/Cargo.toml --bin openpage -- browser logs --session clipboard-probe-20260531 --tail 20`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - both new `protocol.rs` tests passed
  - real CLI output now shows that `browser logs` `content` is filtered through the same mechanism:
    - `_boundary.keys=["content"]`
    - wrapped `OPENPAGE_PAGE_CONTENT ... key=content`
    - truncation marker showing `40 of 92 chars`
  - the runtime sample used:
    - session `clipboard-probe-20260531`
    - persisted log file exists on this machine
  - `openpage_help_marks_tcp_daemon_as_only_active_protocol` passed again
  - removed surfaces still reject with `error.kind="invalid_input"`:
    - `openpage page url`
    - `openpage serve --stdio`
- Interpretation:
  - this is an agent-facing output-shell enhancement, not a browser/runtime change
  - it brings daemon log text under the same trust-boundary model as page text/html output

## Local truth refresh (2026-05-31, competitor borrow doc sync)

- Intent:
  - make the competitor-analysis deliverable directly reusable for future copy/micro-tune work
  - keep the boundary explicit between borrowable shell/control-plane code and non-borrowable browser/runtime code
- Files updated:
  - `竞品文档-考虑借鉴的部分v1.md`
- Added structure:
  - a front-loaded quick-reference table mapping:
    - competitor file
    - recommended borrow action
    - OpenPage landing file
    - hard boundary/rule
- Interpretation:
  - this is documentation hardening, not an implementation change
  - it reduces the risk that a later session copies from `agent-browser` at the wrong layer

## Local truth refresh (2026-05-31, AI-first snapshot shell pass)

- Intent:
  - borrow more of the competitor's agent-facing snapshot contract
  - keep the implementation inside OpenPage's own DOM/JS shell instead of adopting competitor native/CDP internals
- Code changed:
  - `rust/src/cli/serve.rs`
- Behavior now verified:
  - `snapshot` entries can now carry `label`, `checked`, `selected`, and `disabled`
  - text formatting now surfaces those fields in the compact `@eN ...` summary
  - the snapshot pass now clears existing `data-op-ref` attributes before minting the next ref set
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml format_snapshot_text_includes_title_origin_refs_and_attrs -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml snapshot_refs_builds_ref_index -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - runtime smoke with `browser start -> snapshot -> browser stop` on a data URL page containing:
    - labeled email input
    - checked checkbox
    - disabled button
  - runtime smoke with `snapshot -> js mutate -> snapshot -> click @e1 -> js read dataset.clicked`
- Observed truth:
  - real snapshot JSON now included:
    - `label: "Email"`
    - `checked: true`
    - `disabled: true`
  - real compact snapshot text now included:
    - `@e1 [input] label="Email" type="email" placeholder="Work email" id="email"`
    - `@e2 [input] type="checkbox" id="agree" checked`
    - `@e3 [button] "Continue" id="go" disabled`
  - dynamic-page smoke after removing the old element's `role` and `onclick` showed:
    - second snapshot shrank from `interactive_count=2` to `interactive_count=1`
    - `@e1` was reassigned to the remaining live button
    - clicking `@e1` set `dataset.clicked` to `new`
- Interpretation:
  - this is a clean outer-shell borrow point
  - it improves ref lifecycle safety and agent readability without touching browser/CDP/locator/interaction internals

## Local truth refresh (2026-05-31, version-mismatch daemon guard pass)

- Intent:
  - tighten the single active TCP daemon protocol so follow-up `--session` commands cannot keep talking to a live old-version daemon
  - keep the change entirely in the daemon/session control plane
- Code changed:
  - `rust/src/cli/connection.rs`
- Behavior now verified:
  - `ensure_existing_daemon()` now rejects a ready-but-version-mismatched daemon
  - `browser status` / `browser list` now surface:
    - `state="incompatible"`
    - `reasons=["version_mismatch"]`
    - `version_matches_current_cli=false`
  - summary payloads now also count `incompatible`
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml ensure_existing_daemon_rejects_version_mismatch -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_marks_version_mismatch_as_incompatible -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_version_mismatch_as_incompatible -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - runtime smoke under temporary `OPENPAGE_HOME`:
    - `browser start --session version-guard-smoke --headless https://example.com`
    - overwrite `daemon/version-guard-smoke.version` with `0.0.1`
    - `browser status --session version-guard-smoke`
    - `browser list`
    - `title --session version-guard-smoke`
    - `browser stop --session version-guard-smoke`
- Observed truth:
  - before mutation:
    - `browser status` returned `state="healthy"` and `version_matches_current_cli=true`
  - after mutation:
    - `browser status` returned `state="incompatible"` and `reasons=["version_mismatch"]`
    - `browser list` returned `summary.incompatible=1`
    - `title --session version-guard-smoke` failed with restart guidance instead of talking to the stale daemon
- Interpretation:
  - this is a stronger protocol-uniqueness guard, not just a nicer inventory payload
  - it prevents mixed-version daemon follow-up traffic without touching browser/CDP/interaction internals

## Local truth refresh (2026-05-31, doctor quick-fix incompatible-session pass)

- Intent:
  - close the operational gap after version-mismatch fail-fast
  - make `doctor --quick --fix` the unified cleanup path for incompatible daemon sessions too
- Code changed:
  - `rust/src/cli/doctor.rs`
- Behavior now verified:
  - `doctor --quick --fix` now stops incompatible live daemon sessions whose sidecar version does not match the current CLI
  - daemon warning text now makes the state explicit as `state=incompatible`
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incompatible_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incomplete_unready_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml ensure_existing_daemon_rejects_version_mismatch -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - synthetic runtime smoke under temporary `OPENPAGE_HOME`:
    - `browser start --session doctor-fix-mismatch --headless https://example.com`
    - overwrite `daemon/doctor-fix-mismatch.version` with `0.0.1`
    - `doctor --quick`
    - `doctor --quick --fix`
    - `doctor --quick`
    - `browser list`
- Observed truth:
  - before fix:
    - `doctor --quick` reported `daemon.session.doctor-fix-mismatch`
    - its message included `state=incompatible`
    - inventory summary reported `incompatible=1`
  - during fix:
    - `fixed[]` included `Stopped incompatible daemon session doctor-fix-mismatch (found version 0.0.1, current CLI 0.1.0)`
  - after fix:
    - `doctor --quick` reported no daemon warnings for that synthetic home
    - `browser list` returned `sessions=[]`
    - `summary.incompatible=0`
- Interpretation:
  - this completes the control-plane cleanup path for stale daemon version drift
  - it still does not touch browser/CDP/locator/interaction internals

## Local truth refresh (2026-05-31, active-doc sync + browser-logs incompatible test)

- Intent:
  - keep the active skill/install guidance aligned with the new `doctor --quick --fix` behavior
  - add one more shell-level regression guard for incompatible session diagnostics
- Code/docs changed:
  - `rust/src/cli/oneshot.rs`
  - `skills/openpage-test/SKILL.md`
  - `skills/openpage-test/references/install.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Behavior now verified:
  - `browser logs` payload preserves `state="incompatible"` and `reasons=["version_mismatch"]`
  - active usage docs now describe `doctor --quick --fix` as handling incompatible daemon sessions too
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- page url`
  - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- serve --stdio`
- Observed truth:
  - the new `browser_logs_payload_preserves_incompatible_state_and_reasons` test passed
  - removed legacy surfaces still reject with `error.kind="invalid_input"`
  - the active skill/install/smoke docs now explicitly include incompatible daemon sessions in the `doctor --quick --fix` cleanup path
- Interpretation:
  - this is small but important context hygiene for future agent sessions
  - it keeps the repo-local “truth in docs” aligned with the implemented control-plane behavior

## Local truth refresh (2026-05-31, shared session-fix guidance pass)

- Intent:
  - stop making callers reconstruct next actions from `state` and `reasons` alone
  - move session-level fix guidance into the shared control-plane truth
- Code changed:
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/doctor.rs`
  - `rust/src/cli/oneshot.rs`
- Behavior now verified:
  - `browser status` / `browser logs` / `browser list` now preserve a shared machine-readable `fix` string when action is needed
  - `doctor` now reuses the same session-level fix guidance for daemon session warnings instead of maintaining a separate local copy
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_marks_version_mismatch_as_incompatible -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_incomplete_with_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_inactive_when_absent -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_version_mismatch_as_incompatible -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - runtime smoke under temporary `OPENPAGE_HOME`:
    - `browser start --session status-fix-guidance --headless https://example.com`
    - overwrite `daemon/status-fix-guidance.version` with `0.0.1`
    - `browser status --session status-fix-guidance`
    - `browser logs --session status-fix-guidance --tail 5`
    - `browser list`
    - `browser stop --session status-fix-guidance`
- Observed truth:
  - `browser status` returned:
    - `state="incompatible"`
    - `reasons=["version_mismatch"]`
    - `fix="Run openpage browser stop ... restart ..."`
  - `browser logs` preserved the same `fix`
  - `browser list` preserved the same `fix` on the session entry
  - the new logs/state tests passed after the refactor, and the earlier unused-import warning in `doctor.rs` was removed
- Interpretation:
  - this is a stronger agent-facing control-plane borrow point than raw `state` alone
  - it keeps fix guidance consistent across status/logs/list/doctor without touching browser/CDP/locator/interaction internals

## Local truth refresh (2026-05-31, competitor borrow doc rewritten as copy-ready matrix)

- Intent:
  - turn the competitor notes into a practical decision file instead of a loose research memo
  - make future borrowing decisions checkable before code changes
- Files reviewed again:
  - competitor:
    - `参考项目/agent-browser-main/cli/src/connection.rs`
    - `参考项目/agent-browser-main/cli/src/doctor/mod.rs`
    - `参考项目/agent-browser-main/cli/src/doctor/launch.rs`
    - `参考项目/agent-browser-main/cli/src/output.rs`
    - `参考项目/agent-browser-main/cli/src/main.rs`
    - `参考项目/agent-browser-main/cli/src/commands.rs`
  - openpage:
    - `rust/src/cli/connection.rs`
    - `rust/src/cli/doctor.rs`
    - `rust/src/cli/protocol.rs`
    - `rust/src/cli/serve.rs`
    - `rust/src/cli/oneshot.rs`
- Deliverable updated:
  - `竞品文档-考虑借鉴的部分v1.md`
- New structure now captured in that file:
  - borrow boundary table
  - priority matrix
  - competitor file to OpenPage landing map
  - copy-ready vs idea-only vs forbidden split
  - TCP-only adaptation constraints
- Interpretation:
  - the strongest borrow points remain `connection.rs`, `doctor/*`, `output.rs`, top-level error shell, and agent-facing docs
  - `cli/src/native/*` remains outside the allowed borrow boundary

## Local truth refresh (2026-05-31, structured `error.fix` + raw-detail daemon error propagation)

- Intent:
  - tighten the top-level JSON error shell so callers can use machine-readable next-step guidance
  - stop daemon-response errors from drifting through double-prefixed message wrapping on the local CLI path
- Files changed:
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/oneshot.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - top-level direct CLI failures such as inactive/missing session now include `error.fix`
  - version-mismatch follow-up failures now include the full stop/restart guidance in `error.fix`
  - daemon-side `response_openpage_error(...)` now sends raw detail plus optional `fix`, so the local CLI path does not produce a double-prefixed message when reconstructing and reserializing the error
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml rpc_webpage_rejects_inactive_session_without_creating_sidecars -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `OPENPAGE_HOME=/tmp/openpage-cli-missing-fix ... openpage title --session missing`
    - `openpage serve --stdio`
    - `openpage page url`
    - synthetic mismatch session under temporary `OPENPAGE_HOME` with `.version=0.0.1`
- Observed truth:
  - missing session now returns JSON with `error.kind="browser_operation"` and `error.fix="Start it with ..."`
  - version-mismatch follow-up now returns JSON with full `error.fix` containing both stop/restart and doctor cleanup guidance
  - removed legacy surfaces still reject with `error.kind="invalid_input"`
- Interpretation:
  - this is a shell/protocol hardening step, not a browser/runtime change
  - it makes the unique TCP daemon path easier for scripts/agents to consume without string scraping

## Local truth refresh (2026-05-31, omit empty top-level `error.fix`)

- Intent:
  - align top-level direct CLI error JSON with daemon `ResponseError` shape
  - avoid forcing callers to special-case `fix: null`
- Files changed:
  - `rust/src/cli/protocol.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - when a control-plane recovery hint exists, top-level JSON still includes `error.fix`
  - when no such hint exists, top-level JSON now omits `error.fix` instead of emitting `null`
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml simple_error_omits_fix_when_absent -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `openpage title --session missing`
    - `openpage serve --stdio`
    - `openpage page url`
- Observed truth:
  - missing session still returns `error.fix`
  - removed surfaces still reject with `error.kind="invalid_input"`
  - those invalid-input payloads no longer include `fix: null`
- Interpretation:
  - this is another pure protocol-shell cleanup step
  - it keeps the unique TCP daemon JSON surface tighter for automation

## Local truth refresh (2026-05-31, direct error now exposes `state/reasons`)

- Intent:
  - make direct follow-up command failures speak the same control-plane language as `browser status` / `browser logs` / `browser list`
  - keep this strictly in the shell/protocol layer
- Files changed:
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/oneshot.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - known session-control failures now also include top-level `error.state`
  - incompatible failures now also include stable `error.reasons=["version_mismatch"]`
  - inactive failures now include `error.state="inactive"` without a fake `reasons=[]`
  - removed legacy surfaces still reject as plain `invalid_input` without unrelated session state
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml 'state_and_reasons' -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml structured_fix -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `openpage title --session missing`
    - synthetic mismatch session with `.version=0.0.1`
    - `openpage serve --stdio`
- Observed truth:
  - missing session now returns `error.state="inactive"`
  - version mismatch now returns `error.state="incompatible"` + `error.reasons=["version_mismatch"]`
  - removed surfaces still reject with `error.kind="invalid_input"` and no session-state baggage
- Interpretation:
  - this is a stronger machine-readable borrow point than `error.fix` alone
  - it keeps direct failure payloads aligned with the same control-plane truth used elsewhere

## Local truth refresh (2026-05-31, daemon-related `doctor.checks[]` now carry `state/reasons`)

- Intent:
  - align doctor's check-oriented output with the same control-plane truth already used by inventory/status/logs/direct-error
  - keep this scoped to daemon-related checks only
- Files changed:
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/doctor.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - daemon-related doctor checks now also carry machine-readable `state`
  - incompatible daemon checks now also carry `reasons=["version_mismatch"]`
  - incomplete daemon checks now also carry the same stable incomplete-session reason taxonomy
  - non-daemon checks are unchanged
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_state_and_reasons_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke with temporary `OPENPAGE_HOME` and a version-mismatched daemon session
- Observed truth:
  - `openpage doctor --quick` now returns a `daemon.session.*` check with:
    - `state="incompatible"`
    - `reasons=["version_mismatch"]`
    - full `fix`
  - removed protocol surfaces remain rejected and unaffected by this change
- Interpretation:
  - this extends the shared control-plane taxonomy into doctor's primary `checks[]` surface
  - it is still a pure shell/control-plane borrow point

## Local truth refresh (2026-05-31, machine-readable `session` for direct error + doctor daemon checks)

- Intent:
  - stop making callers recover the session name from free-form `message` or from `daemon.session.<name>` ids
  - keep this limited to session/control-plane surfaces
- Files changed:
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/doctor.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - known session-control direct errors now include `error.session`
  - daemon-related doctor checks now include `session`
  - state/reasons/fix behavior from the previous turns remains intact
- Commands run:
  - exact targeted tests only:
    - `simple_openpage_error_exposes_structured_fix_for_session_guidance`
    - `simple_openpage_error_exposes_state_and_reasons_for_version_mismatch`
    - `response_openpage_error_uses_raw_detail_and_structured_fix`
    - `response_result_preserves_structured_fix_without_double_prefix`
    - `response_result_reconstructed_error_keeps_state_and_reasons_for_incompatible_session`
    - `check_serializes_state_and_reasons_when_present`
    - `daemon_checks_include_machine_readable_state_and_reasons`
    - `openpage_help_marks_tcp_daemon_as_only_active_protocol`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `openpage title --session missing`
    - synthetic mismatch session + `openpage doctor --quick`
- Observed truth:
  - missing session now returns `error.session="missing"`
  - synthetic mismatch doctor run now returns a daemon check with `session="session-field-smoke"`
  - removed protocol surfaces remain rejected and unaffected
- Important note:
  - a broad `cargo test ... 'session' ...` filter also matched unrelated pre-existing repo tests and produced noise unrelated to this shell/protocol work; use exact test names for this track
- Interpretation:
  - this is another pure machine-readable control-plane improvement
  - it reduces parsing work for agents/scripts without changing browser/runtime internals

## Local truth refresh (2026-05-31, `doctor.inventory` stays present when daemon dir is absent)

- Intent:
  - make doctor's inventory shape as stable as `browser list`
  - avoid a special `null` branch for the empty/no-daemon-dir case
- Files changed:
  - `rust/src/cli/doctor.rs`
  - `README.md`
  - `skills/openpage-test/references/session-management.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
- New behavior now verified:
  - when the daemon directory does not exist yet, `doctor --quick` now still returns `inventory` as an object
  - that object has zero counts and empty arrays instead of collapsing to `null`
- Commands run:
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_return_empty_inventory_when_daemon_dir_is_missing -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml openpage_help_marks_tcp_daemon_as_only_active_protocol -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime smoke:
    - `OPENPAGE_HOME=/tmp/openpage-doctor-empty-shape openpage doctor --quick`
- Observed truth:
  - `doctor --quick` now returns `inventory.summary.total=0` with empty arrays when no daemon dir exists
  - the surrounding info check `daemon.sessions` still remains intact
- Interpretation:
  - this is another pure machine-friendly output-shape improvement
  - it keeps doctor closer to the same inventory contract as `browser list`

## Competitor-doc refresh (2026-05-31, borrow/copy boundaries rewritten)

- Intent:
  - turn the existing competitor notes into a direct implementation checklist
  - make it explicit what can be copied, what must stay OpenPage-native, and which files are involved
- Files changed:
  - `竞品文档-考虑借鉴的部分v1.md`
  - `task_plan.md`
  - `notes.md`
  - `claude-progress.txt`
- Source-of-truth note:
  - local `.codegraph/codegraph.db` only covers part of the current OpenPage repo and does not index the reference project
  - codegraph also appears stale for at least some current OpenPage files, so final competitor conclusions were taken from real source files, not the graph index alone
- Main output now captured in the doc:
  - first-tier copy candidates:
    - `参考项目/agent-browser-main/cli/src/connection.rs`
    - `参考项目/agent-browser-main/cli/src/doctor/mod.rs`
    - `参考项目/agent-browser-main/cli/src/doctor/launch.rs`
    - selected helper functions from `参考项目/agent-browser-main/cli/src/output.rs`
  - keep-as-OpenPage-native boundary:
    - `参考项目/agent-browser-main/cli/src/native/*`
    - browser/CDP/locator/interaction/snapshot runtime internals
  - landing files on the OpenPage side:
    - `rust/src/cli/connection.rs`
    - `rust/src/cli/doctor.rs`
    - `rust/src/cli/protocol.rs`
    - `rust/src/cli/mod.rs`
    - `rust/src/cli/oneshot.rs`
    - `skills/openpage-test/*`
- Interpretation:
  - the competitor is most useful as a TCP-daemon CLI shell reference, not as a browser runtime reference

## Local truth resync (2026-05-31, runtime-only audit; no code edits)

- Intent:
  - re-sync the repo-local tracking files to the machine's current runtime truth
  - avoid touching `rust/src/cli/oneshot.rs` while local interactive staging is still active
- Commands run:
  - `rust/target/debug/openpage --help`
  - `rust/target/debug/openpage serve --help`
  - `rust/target/debug/openpage browser list`
  - `rust/target/debug/openpage doctor --quick`
  - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage serve --stdio`
  - `rust/target/debug/openpage page url`
- Observed truth:
  - `browser list` currently returns `healthy=18 / incompatible=0 / incomplete=0 / cleaned=0 / total=18`
  - current healthy session names include the prior 17-session set plus `hist-smoke`
  - plain `doctor --quick` currently returns `pass=22 / warn=0 / fail=1 / info=1 / total=24`
  - the only fail is still `browser.executable`, tied to the current machine-local dirty config `browser_path=/tmp/dp-browser`
  - with `OPENPAGE_BROWSER_PATH=/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, `doctor --quick` becomes `pass=23 / warn=0 / fail=0 / info=1 / total=24`
  - removed `serve --stdio` and old `page url` still return the unified JSON shell with `error.kind="invalid_input"`
- Git-safety note:
  - `git add -p rust/src/cli/oneshot.rs` is still active locally
  - this audit deliberately avoided editing `rust/src/cli/oneshot.rs` or doing any broad staging/reset operation
- Files synced from this audit:
  - `task_plan.md`
  - `skills/openpage-test/references/cli-smoke.md`
  - `竞品文档-考虑借鉴的部分v1.md`
  - `notes.md`
  - `claude-progress.txt`
- Next shell-only borrow point after staging is safe:
  - preserve daemon `session/state/reasons/fix` context more directly across `ResponseError -> response_result(...) -> simple_openpage_error(...)`
  - keep this scoped to `rust/src/cli/protocol.rs` and `rust/src/cli/oneshot.rs`

## Doctor browser-path field cleanup (2026-06-01)

- Intent:
  - finish the interrupted `rust/src/cli/doctor.rs` patch without touching runtime internals or `rust/src/cli/oneshot.rs`
  - make doctor browser-path checks more machine-readable for agent/script consumers
- Code change:
  - completed `Check` field initialization for:
    - `browser_path`
    - `resolved_path`
    - `suggested_path`
  - left the runtime/browser/CDP/locator/interaction stack untouched
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml` now passes again
  - default machine state:
    - `cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - `browser.config.browser_path="/tmp/dp-browser"`
    - `browser.executable.browser_path="/tmp/dp-browser"`
  - process-local override state:
    - `OPENPAGE_BROWSER_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' cargo run --manifest-path rust/Cargo.toml --bin openpage -- doctor --quick`
    - `browser.executable.resolved_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
  - hint-path state:
    - temp project dir with `dp_configs.ini` containing `[chromium_options] browser_path = chrome`
    - `/Volumes/data0/data4work/2026_5/openpage/rust/target/debug/openpage doctor --quick`
    - `browser.executable.suggested_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
    - `browser.executable.hint.suggested_path="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`
- Constraints still in force:
  - local `git add -p rust/src/cli/oneshot.rs` remains active
  - no edits were made to `rust/src/cli/oneshot.rs`
- Interpretation:
  - this is a valid non-CDP borrow from the competitor's doctor/control-plane shape
  - the next worthwhile borrow point is still direct-error context preservation in `protocol.rs` / `oneshot.rs`, once the local interactive staging risk is gone

## Direct CLI daemon error context preservation (2026-06-04)

- Intent:
  - continue the shell/control-plane borrow line in `protocol.rs` / `oneshot.rs`
  - reduce direct CLI dependence on daemon message-text heuristics when the daemon already returned structured fields
- Code change:
  - added `openpage_error_from_response_error(...)`
  - added `openpage_error_from_structured_context(...)`
  - `rust/src/cli/oneshot.rs::response_result(...)` now reconstructs from the full `ResponseError`
  - canonical session-state reconstruction now covers:
    - `inactive`
    - `incomplete + daemon_not_ready`
    - `incompatible + version_mismatch`
  - canonical transient reconstruction now covers:
    - `daemon_transient`
    - `retryable=true`
    - `suggested_action=retry_same_command`
  - `openpage_error_fix(...)` now also recognizes canonical session-state messages so synthetic structured reconstructions do not lose `fix`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_state_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_transient_fields_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_incompatible_state_when_message_is_generic -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_transient_retry -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_incompatible_state -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `rust/target/debug/openpage serve --stdio`
  - `rust/target/debug/openpage page url`
- Observed truth:
  - all five targeted tests passed after one mid-pass fix to avoid polluting synthetic `fix` extraction with generic daemon message text
  - `cargo check` passes
  - removed protocol surfaces still return `error.kind="invalid_input"`
- Interpretation:
  - this is the next real step toward making direct CLI errors share the same machine-readable control-plane truth as `browser status` / `browser logs` / `browser list` / `doctor`
  - still no competitor runtime internals were borrowed

## Competitor-doc resync (2026-06-04, machine-truth refresh only)

- Intent:
  - finish the competitor-borrow document as a durable reference, without touching runtime code
  - replace stale 2026-05-31 machine snapshots with current 2026-06-04 runtime truth
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
  - `rust/target/debug/openpage serve --stdio`
  - `rust/target/debug/openpage page url`
  - `rust/target/debug/openpage doctor --quick`
  - `rust/target/debug/openpage browser list`
- Observed truth:
  - TCP daemon remains the only active protocol
  - `serve --stdio` still fails with top-level `error.kind="invalid_input"`
  - `page` is no longer an active top-level subcommand; `page url` now fails at command parsing with `error.kind="invalid_input"`
  - `browser list` currently reports `healthy=0 / incompatible=0 / incomplete=0 / cleaned=18 / total=18`
  - `doctor --quick` currently reports `pass=5 / warn=18 / fail=0 / info=2 / total=25`
  - current browser config resolves to `browser_path=<default>` and `browser.executable` is now informational, not failing
- Files synced:
  - `竞品文档-考虑借鉴的部分v1.md`
  - `task_plan.md`
  - `notes.md`
  - `claude-progress.txt`
- Interpretation:
  - the borrow boundary is unchanged: borrow shell/control-plane patterns, do not borrow competitor runtime internals
  - the document is now aligned to current local truth instead of the older `/tmp/dp-browser` machine snapshot

## Competitor-doc deepening (2026-06-04, function-level migration matrix)

- Intent:
  - turn the competitor-borrow document from a file-level recommendation into a function-level migration checklist
  - make future copy/micro-tuning work executable instead of interpretive
- Evidence inspected:
  - `参考项目/agent-browser-main/cli/src/connection.rs`
  - `参考项目/agent-browser-main/cli/src/doctor/mod.rs`
  - `参考项目/agent-browser-main/cli/src/doctor/launch.rs`
  - `参考项目/agent-browser-main/cli/src/output.rs`
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/doctor.rs`
  - `rust/src/cli/protocol.rs`
  - `rust/src/cli/mod.rs`
  - `rust/src/cli/oneshot.rs`
- Main conclusions:
  - `connection.rs` is still the highest-value borrow point, but OpenPage must not regress from its richer `sessions/incomplete/cleaned` inventory model
  - `doctor.rs` is no longer the main gap; OpenPage already exceeds the competitor in machine-readable payload richness
  - `output.rs` and top-level error shell are already being systemically absorbed via `protocol.rs`
  - one worthwhile future tightening point is converting `cleaned.reason: String` toward a more stable taxonomy without reintroducing competitor runtime internals
- Files synced:
  - `竞品文档-考虑借鉴的部分v1.md`
  - `task_plan.md`
  - `notes.md`
  - `claude-progress.txt`

## Cleaned sidecar reason taxonomy (2026-06-04)

- Intent:
  - turn cleaned stale-sidecar reporting into a stable machine-readable taxonomy without breaking existing human-readable summaries
  - keep the change strictly in the TCP daemon control plane
- Code changes:
  - `rust/src/cli/connection.rs`
    - `CleanedDaemonSession` now carries both:
      - `reason` — human-readable summary such as `invalid pid, missing version`
      - `reasons` — stable taxonomy such as `["invalid_pid", "missing_version"]`
    - inventory payload `cleaned[]` now emits `reasons[]`
    - added `CleanedReason` internal enum plus summary/taxonomy helpers
  - `rust/src/cli/doctor.rs`
    - `daemon.cleaned.*` checks now also carry machine-readable `reasons[]`
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml cleaned_reason_taxonomy_is_stable_and_keeps_human_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - `cargo build --manifest-path rust/Cargo.toml`
  - synthetic runtime audit with fresh `OPENPAGE_HOME`:
    - `openpage browser list`
    - `openpage doctor --quick`
- Observed truth:
  - `browser list` cleaned entries now expose both `reason` and stable `reasons[]`
  - `doctor --quick` cleaned checks now expose `state="cleaned"` plus stable `reasons[]`
  - no runtime/kernel surfaces were touched; the change stays in shell/control-plane only

## Batch invalid-input shell alignment (2026-06-04)

- Intent:
  - align malformed `batch` input with the same machine-readable JSON shell used by top-level CLI parse failures
  - keep semantic workflow restrictions under `unsupported_operation`
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - added `batch_error_payload(...)`
    - malformed nested batch parse errors now return `error.kind="invalid_input"`
    - invalid stdin JSON for `batch` now also returns `error.kind="invalid_input"`
    - batch workflow restrictions such as `batch cannot execute serve` still return `error.kind="unsupported_operation"`
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_uses_invalid_input_for_nested_parse_errors -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_uses_invalid_input_for_invalid_stdin_json -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml batch_error_payload_keeps_unsupported_operation_for_batch_restrictions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage batch "page url"`
    - `printf 'not-json' | rust/target/debug/openpage batch`
- Observed truth:
  - malformed batch parse errors now return `error.kind="invalid_input"`
  - invalid batch stdin JSON now returns `error.kind="invalid_input"`
  - semantic batch restrictions still return `error.kind="unsupported_operation"`

## Invalid-value shell alignment (2026-06-04)

- Intent:
  - keep shrinking places where obvious user-input validation errors accidentally surfaced as `unsupported_operation`
  - preserve `unsupported_operation` only for real workflow/platform restrictions
- Code changes:
  - `rust/src/cli/protocol.rs`
    - a narrow subset of `UnsupportedOperation` details now maps to `error.kind="invalid_input"`
    - top-level JSON shell now uses raw detail text for those `invalid_input` cases instead of the `unsupported operation: ...` prefix
    - `openpage_error_from_kind("invalid_input", ...)` now reconstructs through `UnsupportedOperation` without losing the shell kind on re-serialization
  - `rust/src/cli/oneshot.rs`
    - added round-trip test coverage for daemon `invalid_input` responses
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_invalid_value_unsupported_operation_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_kind_for_invalid_snapshot_mode -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_from_daemon_response -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage zoom in --step 0 --session doc-agent`
- Observed truth:
  - direct invalid value checks such as `zoom in --step 0` now return `error.kind="invalid_input"`
  - daemon-side invalid-input-like `UnsupportedOperation` can now round-trip through the JSON shell without degrading back to `unsupported_operation`
  - real semantic restrictions such as `batch cannot execute serve` still stay `error.kind="unsupported_operation"`

## Daemon-side param validation shell alignment (2026-06-04)

- Intent:
  - continue shrinking obvious input-validation cases that still surfaced as `browser_operation` or `unsupported_operation`
  - keep the scope narrow to clearly-invalid schema/value checks
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `error.kind="invalid_input"` now also covers a narrow set of daemon-side parameter validation details, including:
      - `history index must be >= 1`
      - `select requires one of: ...`
      - `select-range/select-text requires end >= start`
      - `missing param:` / `missing numeric param:` / `missing headers param:`
      - `... must be ...` schema-shape errors from request parsing helpers
    - the JSON shell now emits raw detail text for those `invalid_input` cases rather than `browser operation failed: ...`
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for daemon `invalid_input` response preservation
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_browser_operation_schema_validation_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_uses_invalid_input_kind_for_browser_operation_param_validation -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_param_validation_detail -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage history go 0 --session doc-agent`
- Observed truth:
  - daemon-side parameter validation such as `history go 0` now returns `error.kind="invalid_input"`
  - these invalid-input-like daemon errors now round-trip through the shell without degrading back to `browser_operation`
  - true workflow restrictions still stay under their existing kinds

## Range/empty/missing-param invalid-input alignment (2026-06-04)

- Intent:
  - continue shrinking daemon-side validation cases that still looked like runtime failures even though the user input itself was invalid
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `error.kind="invalid_input"` now also covers a slightly broader but still narrow set of daemon-side validation details, including:
      - `history index out of range: ...`
      - `find-in-page text must not be empty`
      - `missing target`
      - `missing string/number/array param: ...`
    - these errors now render as raw invalid-input messages instead of `browser operation failed: ...`
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for `invalid_input` details such as missing required string params
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_find_in_page_empty_text_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_missing_string_param_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_missing_string_param_detail -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage history go 999999 --session doc-agent`
    - `rust/target/debug/openpage find-in-page "" --session doc-agent`
- Observed truth:
  - out-of-range history selection now returns `error.kind="invalid_input"`
  - empty find-in-page text now returns `error.kind="invalid_input"`
  - missing-param-like daemon validation can now round-trip through the shell without degrading back to `browser_operation`

## Navigation token invalid-input alignment (2026-06-04)

- Intent:
  - continue shrinking stateful-but-user-supplied invalid token cases that still surfaced as `browser_operation`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - bad navigation token details such as `unknown navigation token: ...` now map to `error.kind="invalid_input"`
    - token/frame mismatch details are now grouped into the same invalid-input bucket
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for daemon `invalid_input` preservation on bad navigation tokens
  - docs synced:
    - `README.md`
    - `skills/openpage-test/references/cli-smoke.md`
    - `竞品文档-考虑借鉴的部分v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_unknown_navigation_token_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_preserves_invalid_input_kind_for_unknown_navigation_token -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
  - runtime:
    - `rust/target/debug/openpage wait-for-navigation --token definitely-bad --timeout 1 --session doc-agent`
- Observed truth:
  - bad navigation tokens now return `error.kind="invalid_input"`
  - these token-validation errors no longer degrade into `browser_operation` in the JSON shell

## Invalid-input contract hardening (2026-06-04)

- Intent:
  - harden the current invalid-input shell boundary so later edits do not silently drift kinds back toward `browser_operation` or `unsupported_operation`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - added table-driven contract coverage for the known invalid-input detail taxonomy
    - added explicit negative coverage proving that runtime/state cases such as `unknown target: ...` and real restrictions such as `downloads open is unsupported on this platform` stay out of the invalid-input bucket
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml invalid_input_contract_covers_known_detail_taxonomy -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml invalid_input_contract_keeps_runtime_and_restriction_cases_outside_bucket -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current invalid-input bucket is now protected by a contract test instead of only one-off case tests
  - negative cases still prove the bucket is not swallowing genuine runtime or platform-restriction errors

## Error semantics map artifact (2026-06-04)

- Intent:
  - stop relying on scattered memory of the recent shell/control-plane error-kind tightening work
  - publish a durable map of the current classification boundary
- Artifact created:
  - `错误语义地图-v1.md`
- What it captures:
  - current meaning of `invalid_input`, `unsupported_operation`, `browser_operation`, `daemon_transient`, `invalid_json`, `tcp_error`
  - positive examples already verified at runtime
  - negative examples that should stay outside the invalid-input bucket
  - current contract tests that protect the boundary from drift
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the shell/control-plane error tightening line now has both implementation/tests and a stable human-readable map

## Control-plane map artifact (2026-06-04)

- Intent:
  - complement the error-kind map with a durable module/ownership map for the active TCP CLI shell
  - make later borrow work stay constrained to control-plane files instead of drifting back into runtime internals
- Artifact created:
  - `控制面地图-v1.md`
- What it captures:
  - current roles of `connection.rs`, `doctor.rs`, `protocol.rs`, `oneshot.rs`, and `mod.rs`
  - current control-plane data flow and ownership boundaries
  - which parts are good borrow targets from `agent-browser`, and which parts should stay out of scope
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `ls -l 控制面地图-v1.md`
  - `rg -n "控制面地图-v1.md" README.md skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the shell/control-plane line now has both an error-semantics map and a module-ownership map

## Borrow migration checklist artifact (2026-06-05)

- Intent:
  - turn the competitor-borrow guidance into an executable migration checklist instead of a narrative recommendation
  - keep future borrowing constrained to `rust/src/cli/*` and out of runtime internals
- Artifact created:
  - `借鉴迁移清单-v1.md`
- What it captures:
  - priority order for borrow targets
  - per-file/per-function migration suggestions
  - what can be copied directly vs only referenced
  - mandatory edits after copy
  - minimal verification expectations for each borrow step
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `ls -l 借鉴迁移清单-v1.md`
  - `rg -n "借鉴迁移清单-v1.md" README.md skills/openpage-test/references/cli-smoke.md`
- Interpretation:
  - the competitor-borrow line now has a concrete execution checklist, not just a direction memo

## Daemon transient retry classifier tightening (2026-06-05)

- Intent:
  - borrow one small but high-value control-plane behavior from the competitor without touching runtime internals
  - make daemon retry classification tolerate more startup/restart-adjacent malformed-response cases
- Code changes:
  - `rust/src/cli/connection.rs`
    - `is_transient_error(...)` now also treats these as transient:
      - `EOF while parsing a value`
      - `expected value at line 1 column 0`
      - `line 1 column 0`
      - `Connection aborted`
      - `os error 2`
    - added retry coverage for EOF-like and empty-JSON-like serialization failures
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_retries_after_eof_like_serialization_error -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml send_request_with_retry_retries_after_empty_json_like_serialization_error -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - retry classification is now closer to the competitor's mature shell behavior
  - the change stays fully inside `rust/src/cli/connection.rs`
  - no TCP/runtime/protocol boundary changed

## Existing daemon reuse stability recheck (2026-06-05)

- Intent:
  - borrow the competitor's ready-recheck discipline before reusing an existing daemon
  - reduce the chance of reusing a daemon that is already in the middle of shutting down
- Code changes:
  - `rust/src/cli/connection.rs`
    - `existing_daemon_action_with_retry(...)` now waits briefly and re-checks readiness before returning `Reuse`
    - added `READY_RECHECK_DELAY_MS`
    - if the daemon disappears during that short window, the flow falls back to the normal alive/unready handling path instead of reusing it
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_does_not_reuse_daemon_that_drops_during_recheck_window -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml existing_daemon_action_reuses_ready_matching_daemon -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - stable ready daemons are still reused
  - daemons that vanish during the short recheck window are no longer eagerly reused
  - the change stays inside the TCP control plane and does not touch runtime internals

## Failed startup sidecar cleanup tightening (2026-06-05)

- Intent:
  - tighten the daemon startup failure path so an early child exit does not leave stale sidecars behind until a later inventory sweep
  - keep persisted daemon logs readable while immediately cleaning `.port/.pid/.version`
- Code changes:
  - `rust/src/cli/connection.rs`
    - extracted `startup_exit_error(...)`
    - early daemon startup exits now route through that helper
    - the helper immediately cleans sidecars before building the returned IO error
    - if the daemon exits right after the polling loop ends, the final `try_wait()` now still takes the same cleanup path
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_and_surfaces_log_content -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_without_log_content -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - failed startup now eagerly removes stale `.port/.pid/.version` sidecars
  - startup error messages still preserve log content when available
  - `.log` files are intentionally left intact for later `browser logs` inspection

## Startup-timeout cleanup tightening (2026-06-05)

- Intent:
  - make the startup-timeout path behave more like a failed bootstrap cleanup path instead of leaving a detached startup daemon around after timeout
  - keep the persisted log file as the surviving diagnostic artifact
- Code changes:
  - `rust/src/cli/connection.rs`
    - on startup timeout, `ensure_daemon(...)` now best-effort kills the still-running child handle and waits for it
    - extracted `startup_timeout_error(...)`
    - timeout errors now also clean `.port/.pid/.version` immediately before returning
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml startup_timeout_error_cleans_sidecars_and_preserves_log_path_in_message -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml startup_exit_error_cleans_sidecars_and_surfaces_log_content -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - timeout failures now stop behaving like sidecar-leaking startup attempts
  - the `.log` path is still preserved in the returned error message
  - the surviving diagnostic artifact after timeout is the log file, not stale sidecars

## Startup failure direct-error context preservation (2026-06-05)

- Intent:
  - close a shell-level gap where startup failures kept `error.kind="io"` but lost machine-readable recovery fields
  - keep the kind stable while surfacing `session` and `fix`
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_context(...)` now recognizes startup failure IO details of the form:
      - `daemon for session '...' exited during startup`
      - `daemon for session '...' failed to become ready during startup`
    - those payloads now surface:
      - `error.session`
      - `error.fix`
    - fix points callers at `openpage browser logs --session ... --tail 20`
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_session_and_fix_for_startup_timeout_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_exposes_session_and_fix_for_startup_exit_io -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - startup failures still keep `error.kind="io"`
  - callers now also get machine-readable recovery context instead of only a free-form message

## Generic startup-io round-trip preservation (2026-06-05)

- Intent:
  - close the remaining round-trip gap where a daemon could send a generic startup `io` message plus structured `session/fix`, and `response_result(...)` would otherwise degrade that back to a plain free-form IO string
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_from_structured_context(...)` now canonicalizes generic startup `io` errors into a session-tagged startup-failure form when the structured fix matches the startup-log recovery action
    - `startup_failure_session_from_detail(...)` now also recognizes the canonical `startup failure:` form
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for generic startup `io` daemon responses carrying structured `session/fix`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_startup_failure_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_and_fix_for_generic_startup_failure_io -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - generic startup `io` daemon responses no longer lose `session/fix` when reconstructed through `response_result(...)`
  - the shell kind still remains `io`

## Generic session-io round-trip preservation (2026-06-05)

- Intent:
  - extend the same round-trip discipline beyond startup-specific IO failures
  - ensure a daemon response carrying `kind="io"` plus structured `session` does not degrade back into an unstructured plain IO string
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `openpage_error_from_structured_context(...)` now canonicalizes generic structured session-scoped IO into `daemon for session '...': ...`
    - `openpage_error_context(...)` now extracts `session` from that generic canonical IO form as well
  - `rust/src/cli/oneshot.rs`
    - added round-trip coverage for generic `io` daemon responses carrying `session` and no `fix`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml reconstructs_openpage_error_from_structured_context_for_generic_session_io -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_result_uses_structured_session_for_generic_io_without_fix -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - generic `io` daemon responses now preserve `error.session` across reconstruction
  - this stays a shell/control-plane change only; `error.kind` remains `io`

## Cleaned inventory log diagnostics (2026-06-05)

- Intent:
  - make stale-sidecar cleanup more diagnosable without turning cleaned residue back into active state
  - surface whether a cleaned session still has a persisted daemon log worth inspecting
- Code changes:
  - `rust/src/cli/connection.rs`
    - `CleanedDaemonSession` now also carries:
      - `log_path`
      - `log_exists`
    - `daemon_inventory_payload_json(...)` now emits those fields under `cleaned[]`
  - `rust/src/cli/doctor.rs`
    - cleaned daemon checks now also retain `log_path`
  - `rust/src/cli/oneshot.rs`
    - browser inventory payload tests updated to assert the new cleaned log diagnostics
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `cleaned[]` now tells callers whether a stale cleaned session still has a persisted log
  - the session remains `state="cleaned"`; this is extra diagnostics, not a state reclassification

## Cleaned inventory machine-readable fix (2026-06-05)

- Intent:
  - make cleaned residue actionable for automation instead of only descriptive
  - align cleaned entries with the rest of the control plane, where payloads usually carry a next-step hint
- Code changes:
  - `rust/src/cli/connection.rs`
    - added `cleaned_daemon_fix(...)`
    - `cleaned[]` payload entries now include `fix`
    - when a cleaned log still exists, the fix points to `browser logs --session ... --tail 20`
    - when no log exists, the fix falls back to restarting the session if needed
  - `rust/src/cli/doctor.rs`
    - cleaned daemon checks now also carry `fix`
  - `rust/src/cli/oneshot.rs`
    - browser inventory payload tests updated to assert cleaned fixes
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - cleaned residue is now both diagnosable and actionable
  - this stays a control-plane guidance change; cleaned sessions are still not treated as active

## Doctor cleaned-check log contract alignment (2026-06-05)

- Intent:
  - finish aligning doctor cleaned checks with the cleaned inventory payload shape
  - make stale-log existence machine-readable in doctor output, not just in inventory
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `Check` now also supports `log_exists`
    - cleaned daemon checks now emit both `log_path` and `log_exists`
    - daemon-check shape test now includes a real cleaned fixture and asserts the cleaned fix/log fields
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - doctor cleaned checks now expose the same stale-log existence signal as inventory
  - this improves automation/debuggability without changing any session state classification

## Active/incomplete daemon log contract alignment (2026-06-05)

- Intent:
  - finish the daemon log diagnostics alignment across `browser list`, `browser status`, and `doctor`
  - stop treating `log_exists` as a cleaned-only signal when active/incomplete sessions also have a persisted log truth
- Code changes:
  - `rust/src/cli/connection.rs`
    - added `log_exists` to `DaemonSessionInfo` and `IncompleteDaemonSession`
    - `daemon_status(...)` and `daemon_inventory(...)` now capture log presence for active/incomplete sessions
    - `daemon_inventory_payload_json(...)` now emits `log_exists` for `sessions[]` and `incomplete[]`
  - `rust/src/cli/doctor.rs`
    - daemon-session and incomplete-session checks now emit `log_exists`
  - `rust/src/cli/oneshot.rs`
    - browser inventory tests updated to assert the aligned payload shape
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_incomplete_with_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_daemon_runtime_fields_when_present -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - active, incomplete, and cleaned daemon surfaces now all expose log availability explicitly
  - this is a control-plane shape alignment only; it does not change daemon lifecycle behavior

## Browser logs log-existence contract alignment (2026-06-05)

- Intent:
  - finish the daemon log-shape alignment by making `browser logs` expose the same `log_exists` signal as `browser status` / `browser list` / `doctor`
  - keep backward compatibility for existing callers that still read `exists`
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - `run_browser_logs(...)` now prefers structured `log_exists` from the status payload and only falls back to `Path::exists()` when needed
    - `browser_logs_payload(...)` now emits `log_exists` and keeps `exists` as a compatibility alias
    - added a false-case test so the inactive/no-log shape stays explicit and machine-readable
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_false_log_exists -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `browser logs` now speaks the same log-availability contract as the rest of the daemon control plane
  - the legacy `exists` field still works, but it is now clearly just an alias for `log_exists`

## Doctor auto-fix contract tightening (2026-06-05)

- Intent:
  - make `summary.fixable_ids` reflect the real scope of `doctor --quick --fix` instead of every check that merely carries manual guidance
  - separate machine-readable auto-fixability from human-readable `fix` text
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added `auto_fixable=true` for checks that `apply_fixes()` can actually repair automatically
    - `summary.fixable` / `summary.fixable_ids` now count only those checks
    - legacy-session residue, incompatible daemon sessions, and incomplete unready daemon sessions are now the explicit auto-fix bucket
    - manual guidance checks such as browser executable / browser launch keep `fix` text but are no longer misclassified as auto-fixable
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml summarize_counts_info_fixable_and_total -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_auto_fixable_only_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `checks[].fix` now clearly means “guidance exists”, while `fixable_ids` means “doctor can do it for you”
  - this removes a shell-level ambiguity that would otherwise mislead automation

## Doctor fixed[] structure alignment (2026-06-05)

- Intent:
  - make `doctor --quick --fix` results machine-readable end-to-end instead of leaving `fixed[]` as free-form strings
  - align applied-fix reporting with `checks[].id` and `summary.fixable_ids`
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added structured `FixedAction` entries with `check_id`, `message`, `auto_fixable`, and optional `session` / `path`
    - `apply_fixes()` now returns structured entries for legacy JSON cleanup, incompatible daemon cleanup, incomplete unready daemon cleanup, and opportunistic stale-sidecar cleanup
    - stale-sidecar cleanup is explicitly represented as `auto_fixable=false` because it happens during inventory scan, not through a directly fixable check
  - tests now assert the new structure instead of only string containment
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml remove_legacy_session_files_deletes_only_json_entries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incomplete_unready_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_stops_incompatible_daemon_session -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml fixed_action_serializes_machine_fields -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `fixed[]` can now be consumed by scripts without scraping human text
  - `check_id` now closes the loop between checks, fixable summary, and applied-fix reporting

## Doctor --fix post-fix view contract (2026-06-05)

- Intent:
  - remove ambiguity around whether `doctor --quick --fix` returns a pre-fix snapshot or a post-fix snapshot
  - verify that applied-fix reporting and current-state reporting can be consumed together safely
- Code changes:
  - `rust/src/cli/doctor.rs`
    - extracted `doctor_payload(&DoctorArgs)` so the JSON report can be tested directly
    - added a regression test that sets up legacy residue, stale sidecars, an incomplete unready daemon, and an incompatible daemon, then verifies:
      - `fixed[]` reports all applied actions
      - `summary.fixable_ids` is empty after cleanup
      - `inventory.summary` is the post-fix zero-residue view
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - `doctor --quick --fix` now has an explicit tested contract: applied fixes are listed in `fixed[]`, while `summary` / `checks` / `inventory` describe the resulting post-fix state

## Doctor fixed[] source/reason taxonomy (2026-06-05)

- Intent:
  - remove the last ambiguity in structured `fixed[]` output so callers no longer infer cleanup provenance from `auto_fixable` plus free-form text
  - make opportunistic inventory cleanup and direct `--fix` actions distinguishable by stable fields
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `FixedAction` now also carries stable `source` and `reason`
    - `source` currently distinguishes `direct_fix` vs `inventory_scan`
    - `reason` currently distinguishes `legacy_session_json`, `incompatible_daemon`, `incomplete_unready_daemon`, and `stale_sidecars`
    - tests updated so the structured applied-fix contract is asserted directly
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml fixed_action_serializes_machine_fields -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml remove_legacy_session_files_deletes_only_json_entries -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml apply_fixes_reports_stale_daemon_sidecar_cleanup -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml doctor_payload_with_fix_reports_post_fix_inventory_and_structured_fixed_actions -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - callers can now tell whether a `fixed[]` entry came from explicit `--fix` work or opportunistic daemon inventory cleanup without scraping `message`

## Doctor checks[] kind field for daemon sessions (2026-06-05)

- Intent:
  - remove one more place where callers had to infer semantics from `category + id`
  - give concrete daemon-session checks a stable, directly filterable shape marker
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `Check` now supports optional `kind`
    - concrete daemon-session checks now emit `kind="daemon_session"` for healthy/incompatible, incomplete, and cleaned daemon session entries
  - tests updated to assert the new field on serialized checks and daemon-check output
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_state_and_reasons_when_present -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_include_machine_readable_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - consumers can now filter concrete daemon-session checks directly by `kind` instead of recovering that meaning from string prefixes

## Doctor check kinds for core non-daemon checks (2026-06-05)

- Intent:
  - keep pushing `doctor checks[]` away from string-prefix parsing
  - cover the highest-value non-daemon checks with stable kinds before broadening further
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `env.legacy_sessions` now emits `kind="legacy_sessions"`
    - `browser.config` now emits `kind="browser_config"`
    - `browser.executable` and `browser.executable.hint` now emit `kind="browser_executable"`
    - `browser.launch` now emits `kind="browser_launch"`
  - added focused tests for `environment_checks(...)` and `browser_checks(...)` to assert the new kinds on real generated checks
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml environment_checks_include_legacy_sessions_kind -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_checks_include_stable_kinds_for_core_browser_checks -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml check_serializes_auto_fixable_only_when_present -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - callers can now filter the most important non-daemon checks directly by `kind` instead of depending on `id` naming conventions

## Doctor kind coverage for foundational env/daemon checks (2026-06-05)

- Intent:
  - finish the highest-value baseline kinds so automation can classify the doctor control-plane entry points without parsing `id`
  - keep the change scoped to foundational env/daemon checks only
- Code changes:
  - `rust/src/cli/doctor.rs`
    - `env.openpage_home` now emits `kind="openpage_home"`
    - `env.daemon_dir` and `daemon.dir` now emit `kind="daemon_dir"`
    - `daemon.sessions` now emits `kind="daemon_sessions"`
  - focused tests now assert these kinds on real generated checks
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml environment_checks_include_legacy_sessions_kind -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_checks_return_empty_inventory_when_daemon_dir_is_missing -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the doctor shell contract now covers the main env/daemon/browser entry checks with explicit stable `kind` values instead of relying on `id` naming patterns

## Doctor contract inventory doc (2026-06-05)

- Intent:
  - freeze the current doctor JSON shell contract into one place so later tightening work has a stable baseline
  - reduce future context rebuilding when iterating on `doctor` machine fields
- Docs added:
  - `doctor-契约盘点-v1.md`
    - top-level shape
    - `summary` semantics
    - `checks[]` fields and stable `kind` taxonomy
    - `fixed[]` fields plus `source` / `reason`
    - `inventory` shape
    - post-fix view semantics
    - recommended parse order for automation
  - `README.md` now links to the contract inventory doc
- Verification:
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current doctor shell contract is now documented as an explicit artifact instead of being spread across code, tests, and changelog notes

## Doctor kind coverage source-level guard (2026-06-05)

- Intent:
  - turn the current manual kind-coverage audit into an enforceable regression guard
  - prevent future production `doctor` checks from silently landing without `kind`
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added a source-level test that scans the production segment of `doctor.rs` and asserts every `Check::new(...)` block includes `.with_kind(...)`
- Docs synced:
  - `doctor-契约盘点-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml production_check_builders_all_include_kind -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - doctor kind coverage is no longer just convention; it is now guarded by a regression test

## Doctor contract closure conclusion + documented kind baseline (2026-06-05)

- Intent:
  - finish the current doctor-contract tightening stage with an explicit stabilized-vs-unpromised conclusion
  - guard not only that production checks have `kind`, but that the current stable kind baseline matches documentation
- Code changes:
  - `rust/src/cli/doctor.rs`
    - added `production_check_kinds_match_documented_stable_set`
    - this test now guards the current production kind baseline:
      - `openpage_home`
      - `daemon_dir`
      - `legacy_sessions`
      - `daemon_sessions`
      - `daemon_session`
      - `browser_config`
      - `browser_executable`
      - `browser_launch`
- Docs added/updated:
  - `doctor-契约收口结论-v1.md`
    - stable fields
    - stable kind baseline
    - stable fixed/source/reason semantics
    - post-fix view semantics
    - explicitly unpromised areas
  - `doctor-契约盘点-v1.md`
    - now notes that production `Check::new(...)` coverage and kind baseline are source-level guarded
  - `README.md`
    - now links to the closure/conclusion doc
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml production_check_kinds_match_documented_stable_set -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the current doctor shell contract stage now has both a baseline contract inventory and a closure document stating what is stable vs explicitly unpromised

## Browser daemon payload kind alignment (2026-06-05)

- Intent:
  - align `browser status`, `browser logs`, and `browser list` with the doctor daemon-session kind taxonomy
  - let callers filter daemon payloads across these browser surfaces without inferring from field combinations
- Code changes:
  - `rust/src/cli/connection.rs`
    - `daemon_inventory_payload_json(...)` now emits `kind="daemon_session"` for `sessions[]`, `incomplete[]`, and `cleaned[]`
    - `daemon_status_payload_json(...)` now emits `kind="daemon_session"` on the top-level payload and nested `incomplete` payloads
  - `rust/src/cli/oneshot.rs`
    - `browser_logs_payload(...)` now backfills `kind="daemon_session"` when older callers/tests pass a status payload without it
- Docs synced:
  - `README.md`
  - `skills/openpage-test/references/cli-smoke.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml daemon_inventory_payload_json_includes_states_and_summary -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml daemon_status_payload_json_marks_incomplete_with_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_inventory_payload_includes_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_state_and_reasons -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_preserves_incompatible_state_and_reasons -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - the daemon-session kind taxonomy now spans both doctor checks and browser daemon control payloads

## Browser daemon contract inventory doc (2026-06-05)

- Intent:
  - give `browser status` / `browser logs` / `browser list` the same explicit contract treatment that `doctor` now has
  - freeze the current daemon-session field alignment in one place for upper-layer consumers
- Docs added/updated:
  - `browser-daemon-契约盘点-v1.md`
    - stable fields for list/status/logs
    - stable state set
    - stable reasons
    - `kind="daemon_session"` alignment
    - compatible alias notes for `path` / `exists`
    - unpromised ranges
  - `README.md` now links to the browser daemon contract doc
- Code changes:
  - `rust/src/cli/oneshot.rs`
    - added `browser_logs_payload_backfills_daemon_session_kind_when_missing`
    - this locks in backward-compatible `kind` backfill for old status shapes passed into browser-logs payload composition
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml browser_logs_payload_backfills_daemon_session_kind_when_missing -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - browser daemon shell outputs now have a dedicated contract artifact instead of being implied through scattered tests and README bullets

## Missing/shape invalid-input fixes (2026-06-05)

- Intent:
  - extend `error.fix` to one more bounded `invalid_input` family where the recovery path is already explicit from the validation message
- Code changes:
  - `rust/src/cli/protocol.rs`
    - `known_invalid_input_fix(...)` now also covers missing-param and payload-shape details:
      - `missing target`
      - `missing string/number/numeric/array/headers/param`
      - `must be a string or string array`
      - `must be an integer or integer array`
      - `must be an object`
      - `array param must contain only strings/integers`
      - `header values must be strings`
- Docs synced:
  - `README.md`
  - `错误语义地图-v1.md`
- Verification:
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_maps_missing_string_param_to_invalid_input -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml simple_openpage_error_exposes_fix_for_missing_headers_param -- --nocapture`
  - `cargo test --manifest-path rust/Cargo.toml response_openpage_error_exposes_fix_for_object_shape_validation -- --nocapture`
  - `cargo check --manifest-path rust/Cargo.toml`
- Observed truth:
  - covered missing/shape validation errors now surface machine-readable recovery hints instead of requiring callers to infer the next step from free-form detail text

## CLI dogfooding: default dynamic debug ports with persistent profiles (2026-06-06)

- Intent:
  - validate the highest-leverage CLI UX fix from local dogfooding instead of only recommending it
  - remove the default multi-session port collision while keeping session-scoped persistent browser profiles
- Code changes:
  - `rust/src/config.rs`
    - completed `ResolvedConfig.debugger_source` tracking and added a focused test for debugger endpoint source resolution
  - `rust/src/cli/serve.rs`
    - added a narrow runtime policy: when daemon-backed session launch is still using the built-in debugger default and the caller did not pass `--port`, switch launch to `set_local_port(0)`
    - kept the existing session-scoped profile assignment, so this uses Chrome's dynamic port allocation without inheriting `auto_port=true` temp-profile semantics
- Verification:
  - focused tests:
    - `cargo test -p openpage_rs resolved_config_tracks_debugger_source -- --nocapture`
    - `cargo test -p openpage_rs apply_runtime_default_debugger_port -- --nocapture`
  - local install:
    - `cargo install --path . --bin openpage --root /tmp/openpage-cli-eval --force`
  - installed-binary dogfooding:
    - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-default-policy.0dbX1j`
    - `openpage browser start --session default-a --headless https://example.com` => `port=50904`
    - `openpage browser start --session default-b --headless https://example.com` => `port=50905`
    - `openpage title --session default-a` => `Example Domain`
    - `openpage title --session default-b` => `Example Domain`
    - `openpage browser list` => `healthy=2`, `incomplete=0`
    - `openpage doctor --quick` => both sessions reported healthy
    - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-default-persist.fVav4K`
    - `openpage browser start --session persist-a --headless https://example.com` => `port=51131`
    - `openpage js 'localStorage.setItem(...); localStorage.getItem(...)' --session persist-a` => `"default-port-ok"`
    - confirmed persistent profile directory exists under `profiles/persist-a`
    - `openpage browser stop --session persist-a`
    - `openpage browser start --session persist-a --headless https://example.com` => `port=51348`
    - `openpage js 'localStorage.getItem(...)' --session persist-a` => `"default-port-ok"`
- Observed truth:
  - the default daemon-backed CLI startup path no longer collides on the built-in `127.0.0.1:9222` debugger endpoint when opening multiple sessions
  - the fix preserves session persistence semantics because it uses dynamic debug port allocation via `--port 0` behavior rather than `auto_port=true`
  - the top remaining UX work is now mostly explanation/discoverability, not core launch behavior correctness

## CLI dogfooding: long-running navigation blocks daemon control plane (2026-06-06)

- Intent:
  - keep dogfooding past the launch path and identify the next highest-value CLI optimization projects from real interactive usage
- Installed binary used:
  - `/tmp/openpage-cli-eval/bin/openpage`
- Runtime evidence:
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-workflow.C8X0tB`
  - `openpage browser start --session flow --headless https://www.wikipedia.org`
    - command stayed hung
    - concurrent `openpage browser list` reported:
      - `session=flow`
      - `state="incomplete"`
      - `reasons=["daemon_unresponsive"]`
    - concurrent `openpage browser status --session flow` also reported `daemon_unresponsive`
    - `openpage browser logs --session flow --tail 50` showed an empty log file, so the process was alive but not writing stderr
    - process table showed:
      - daemon alive: `openpage serve --port 0 --session flow`
      - Chrome alive with `--remote-debugging-port=0`
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-nav.PDe8D8`
  - `openpage browser start --session nav --headless` succeeded immediately
  - `openpage goto --session nav https://www.wikipedia.org`
    - command stayed hung
    - concurrent `openpage browser list` again reported:
      - `session=nav`
      - `state="incomplete"`
      - `reasons=["daemon_unresponsive"]`
    - `openpage browser stop --session nav` also hung while the navigation request was in flight
- Code evidence supporting the observed duration:
  - `rust/src/page.rs`
    - default page load timeout is `30_000ms`
    - default navigation retry policy is `retry_times=3`, `retry_interval_millis=2_000`
    - navigation path retries `goto_once(...)` serially
  - this means one bad navigation can occupy the daemon for roughly two minutes before surfacing a result
- Contract mismatch found during the same pass:
  - `openpage goto --help` says omitting `--wait` should return a follow-up `wait_for_navigation.command`
  - `rust/src/cli/oneshot.rs::run_goto(...)` always blocks on `rpc_webpage(..., "webpage.get", ...)`
  - `webpage.get` synchronously calls `page.get(...)`
  - so current `goto` is blocking even when `--wait` is omitted; the help text overpromises async behavior
- Additional recovery-surface evidence:
  - `openpage help browser stop` exposes only `--session` and `--all`
  - `openpage browser stop --force --session flow` is rejected as invalid input
  - when the daemon is monopolized by navigation, plain `browser stop` is not a reliable escape hatch
- Lower-priority but real output issue found during the same dogfood pass:
  - firing `snapshot --session batch` before `browser start --session batch` finished produced an `unknown target` error whose `error.fix` text was duplicated multiple times in the JSON payload
- Observed truth:
  - the next highest-value CLI optimization project is no longer startup policy; it is request-lifecycle isolation for long-running navigation
  - current health semantics collapse "daemon is busy inside a long request" and "daemon is unhealthy" into the same `daemon_unresponsive` bucket
  - current recovery UX is incomplete because the CLI lacks an explicit force-stop/kill path for an unresponsive session
  - `batch` is functionally usable for short flows, so its remaining issue is readability rather than basic capability

## CLI dogfooding: root cause pinned to single-client serial daemon loop (2026-06-06)

- Intent:
  - move from symptom collection to a concrete implementation-level root cause for the next CLI optimization stream
- Code evidence:
  - `rust/src/cli/serve.rs`
    - `run_tcp(...)` accepts connections in a plain `for stream in listener.incoming()` loop
    - each accepted connection is handled inline by `handle_client(...)`
    - `handle_client(...)` reads one request line, takes `runtime.borrow_mut()`, and runs `runtime.dispatch(request)` synchronously before accepting any new client
  - this means one long-running RPC monopolizes the entire daemon:
    - no second client can be accepted while the first request is still inside `dispatch(...)`
    - health probes and stop/status/list requests arrive on fresh TCP connections and therefore stall behind the in-flight request
- Controlled local repro:
  - started a local slow HTTP server on `127.0.0.1:55462` that sleeps `45s` before returning
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-localhang.JvhB1O`
  - `openpage browser start --session slow --headless` => immediate success
  - `openpage goto --session slow http://127.0.0.1:55462` => hung
  - concurrent `openpage browser list` => `state="incomplete"`, `reasons=["daemon_unresponsive"]`
  - concurrent `openpage browser status --session slow` => same `daemon_unresponsive`
  - process table confirmed:
    - daemon process alive: `openpage serve --port 0 --session slow`
    - browser alive: Chrome on persistent profile
- Timeout/retry evidence explaining the severity:
  - `rust/src/cli/connection.rs`
    - health probe timeout is only `750ms`
    - normal RPC read timeout is `30s`
  - `rust/src/page.rs`
    - `Page::goto(...)` retries serially
  - `rust/src/browser.rs`
    - default `retry_times=3`, `retry_interval_millis=2000`
  - because the local server sleeps `45s`, each navigation attempt overruns the `30s` page-load window and gets retried, keeping the daemon monopolized far longer than a single slow response
- Recovery-path evidence:
  - `rust/src/cli/connection.rs::shutdown_daemon(...)` does have a forced-kill fallback after a normal shutdown RPC attempt
  - but `browser stop` must first wait for that normal RPC path to timeout, so there is still no immediate operator-controlled force-stop path
  - runtime evidence from the same repro:
    - the hanging `browser stop --session slow` eventually returned `{"forced": true, "had_daemon": true, "stopped": true}`
- Observed truth:
  - the main optimization project is not "navigation is slow"; it is "the daemon concurrency model lets one slow request masquerade as daemon failure"
  - any fix that only tunes timeouts without changing request isolation will leave the control-plane freeze intact

## CLI dogfooding: concurrency feasibility is real, not hypothetical (2026-06-06)

- Intent:
  - verify whether the likely higher-value fixes are blocked by Rust type/threading constraints before recommending them as the main optimization stream
- Scratch compile check:
  - created a temporary crate under `/tmp/openpage-sendcheck.Voa6Yr`
  - compile-time assertions:
    - `assert_send_sync::<openpage_rs::browser::Browser>()`
    - `assert_send_sync::<openpage_rs::page::Page>()`
    - `assert_send_sync::<openpage_rs::webpage::WebPage>()`
  - verification:
    - `cargo check` succeeded
- Supporting source evidence:
  - `rust/src/browser.rs`
    - `Browser` wraps `Arc<BrowserState>`
    - `BrowserState` already uses `Arc<Runtime>` plus mutex-protected internals
  - `rust/src/page.rs`
    - `Page` is `#[derive(Clone)]` and holds `Arc<Runtime>` plus cloneable page handles
  - `rust/src/webpage.rs`
    - `WebPage` is `#[derive(Clone)]` and composes `Browser`, `Page`, `SessionPage`, and `Arc<Mutex<WebMode>>`
  - dependency evidence:
    - chromiumoxide's `Page` is itself `#[derive(Clone)]` over `Arc<PageInner>`
- Practical implication:
  - a deeper fix such as background navigation work plus a responsive control-plane path is technically viable within the current ownership model
  - the hard part is request coordination and session state semantics, not Rust sendability
- Observed truth:
  - project (1) should stay at the top of the roadmap because it is both high-value and implementable
  - the most pragmatic sequencing is:
    1. explicit busy-state instrumentation
    2. first-class force-stop path
    3. real async/non-blocking navigation path
    4. only then, if needed, broader daemon concurrency refactor

## CLI dogfooding: async navigation can reuse existing token/wait protocol (2026-06-06)

- Intent:
  - determine whether "true non-blocking goto" needs a new protocol surface or can be built on existing primitives
- Code evidence:
  - `rust/src/cli/serve.rs`
    - `ServeWebPage` already stores `navigation_tickets` and `next_navigation_ticket_id`
    - `record_navigation_baseline()` already produces stable `nav-*` tokens
    - many existing operations (`click`, `back`, `reload`, `webpage.get`, etc.) already emit `navigation_token`
    - `wait.navigation` already resolves those tokens through `wait_for_navigation_payload(...)`
  - `rust/src/cli/oneshot.rs`
    - `with_navigation_followup(...)` already formats a human-usable `openpage wait-for-navigation --session ... --token ...` follow-up command
- Practical implication:
  - the protocol pieces for async navigation already exist
  - the missing piece is execution model:
    - return the token before blocking navigation finishes
    - run the actual navigation work in a background job or other non-blocking path
    - mark the session as busy while that job owns the page
- Observed truth:
  - project (3) is smaller than a net-new feature because it can reuse existing tokens, wait logic, and follow-up output

## CLI dogfooding: browser start with URL shares the same blocking failure mode (2026-06-06)

- Intent:
  - close the loop on whether the blocking problem is limited to `goto` or also affects `browser start <url>`
- Code evidence:
  - `rust/src/cli/oneshot.rs::start_browser(...)`
    - creates the session first
    - then, when `args.url` is present, immediately calls synchronous `rpc_webpage(..., "webpage.get", ...)`
  - so `browser start <url>` is not a separate startup pathway; it re-enters the same blocking navigation path as `goto`
- Controlled runtime repro:
  - started a local slow HTTP server on `127.0.0.1:60714` that sleeps `45s`
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-starturl.D4Cnet`
  - ran:
    - `openpage browser start --session slowstart --headless http://127.0.0.1:60714`
  - concurrent `openpage browser list` reported:
    - `session=slowstart`
    - `state="incomplete"`
    - `reasons=["daemon_unresponsive"]`
  - concurrent `openpage browser status --session slowstart` also reported `daemon_unresponsive`
- Observed truth:
  - the blocking/navigation-control-plane issue spans both user entry points:
    - `goto`
    - `browser start <url>`
  - any contract or implementation fix must cover both commands together

## CLI dogfooding: smallest high-value first step is likely sidecar-backed busy state (2026-06-06)

- Intent:
  - find the narrowest change that materially improves operator truthfulness before a larger daemon scheduling rewrite
- Code evidence:
  - current daemon inventory/status already relies heavily on filesystem sidecars:
    - `{session}.port`
    - `{session}.pid`
    - `{session}.version`
    - log path discovery
  - these are read from `rust/src/cli/connection.rs` without needing a responsive daemon RPC
- Practical design implication:
  - a new sidecar such as `{session}.activity` / `{session}.busy` can be written before a long request begins and cleared when it ends
  - `browser list`, `browser status`, and `doctor` could surface:
    - `state="busy"` or similar
    - current operation kind, start timestamp, maybe target URL
  - this would avoid misclassifying "daemon is occupied by navigation" as "daemon is unhealthy"
- Sizing judgment:
  - likely a small-to-medium change compared with a full accept-loop / request-scheduler rewrite
  - likely touches:
    - `rust/src/cli/serve.rs`
    - `rust/src/cli/connection.rs`
    - `rust/src/cli/doctor.rs`
- Observed truth:
  - this is the strongest fast-win candidate discovered so far:
    - high user-facing value
    - low conceptual blast radius
    - does not block later async-navigation work

## CLI dogfooding: force-stop is more than a CLI flag because forced cleanup can orphan Chrome (2026-06-06)

- Intent:
  - validate the real implementation size of the "explicit force-stop" project instead of assuming it is a trivial surface-only change
- Runtime evidence:
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-stoptime.MuoNQh`
  - local slow server slept `90s`
  - `openpage goto --session stoptime http://127.0.0.1:62177` put the session into the busy/unresponsive state
  - measured:
    - `OPENPAGE_HOME=/tmp/openpage-dogfood-stoptime.MuoNQh /usr/bin/time -p openpage browser stop --session stoptime`
    - result: `{"forced":true,"had_daemon":true,"session":"stoptime","stopped":true}`
    - wall time: `real 32.39`
  - after that forced return:
    - `browser list` was empty
    - but a Chrome process still existed for `profiles/stoptime`
    - it had to be killed manually
- Code evidence explaining the orphan risk:
  - `rust/src/cli/connection.rs::kill_stale_daemon(...)` kills the pid stored in `{session}.pid`
  - that sidecar pid is the daemon pid, not the Chrome child pid
  - `rust/src/browser.rs`
    - `BrowserState` does track `browser_pid`
    - but that pid is not currently exported through daemon sidecars
  - `BrowserState::drop` only aborts handler tasks; it does not itself close the browser child
- Practical implication:
  - exposing `browser stop --force` naively would improve latency but would also make orphan-browser leaks easier to trigger
  - a correct force-stop project likely needs one of:
    - browser-child pid sidecar(s)
    - daemon-managed kill-both semantics
    - stronger parent/child cleanup guarantees on forced daemon death
- Additional guardrail evidence:
  - `doctor --quick --fix` did not auto-kill the busy session because doctor only auto-fixes incomplete sessions with `ready=false`
  - so current doctor behavior is conservative here, even though it still misclassifies the state as `daemon_unresponsive`
- Observed truth:
  - the operator-visible need for force-stop is real and urgent
  - but the project is medium-sized, not tiny, because it must solve both latency and orphan cleanup

## CLI dogfooding: busy-state touches more surfaces than only browser list/status (2026-06-06)

- Intent:
  - avoid underestimating the blast radius of the top-priority busy/activity project
- Confirmed user-visible surfaces:
  - `rust/src/cli/connection.rs`
    - daemon inventory summary/payload
    - daemon status payload
    - browser logs payload fix text
  - `rust/src/cli/doctor.rs`
    - daemon incomplete/runtime health checks
  - `rust/src/cli/args.rs`
    - `browser start` follow-up help
    - `goto` follow-up help
  - `README.md`
    - current daemon-backed one-command-at-a-time usage examples
- Practical implication:
  - the top project still has bounded code scope
  - but it is not purely internal; payload schema, fix text, and docs/help need to move together
- Observed truth:
  - the smallest coherent ship unit is not just "write a busy sidecar"
  - it is "write busy state + teach status/doctor/help to describe it correctly"

## CLI dogfooding: busy sessions need an error-layer contract too (2026-06-06)

- Intent:
  - determine whether a future busy/activity project can stop at inventory/status surfaces or must also change ordinary command failures
- Controlled busy-session matrix:
  - local slow server on `127.0.0.1:64968` sleeping `45s`
  - fresh `OPENPAGE_HOME=/tmp/openpage-dogfood-busymatrix.rxTxSG`
  - active long request:
    - `openpage goto --session matrix http://127.0.0.1:64968`
  - concurrent command results:
    - `browser status --session matrix`
      - returned `state="incomplete"`, `reasons=["daemon_unresponsive"]`
    - `browser logs --session matrix --tail 5`
      - returned the same `daemon_unresponsive` classification plus empty-log hint
    - `title --session matrix`
      - eventually failed with:
        - `kind="daemon_transient"`
        - message including `io error: Resource temporarily unavailable (os error 35)`
        - fix: `Retry the same command.`
    - `snapshot --session matrix`
      - same `daemon_transient` / `Retry the same command.` result
- Practical implication:
  - today's user experience is internally inconsistent during a busy session:
    - inventory-style commands say "unresponsive"
    - ordinary page commands say "transient IO error"
  - a correct busy-state project should include a dedicated user-facing error shape for ordinary commands, not just a new state in `browser list/status`
- Observed truth:
  - "busy" is both a state-model project and an error-semantics project
  - otherwise the CLI will still feel broken even if inventory payloads become truthful

## CLI dogfooding: Busy Slice A installed-binary confirmation (2026-06-06)

- Landed the narrow truthfulness slice in:
  - `rust/src/cli/connection.rs`
  - `rust/src/cli/protocol.rs`
- Added shell-layer regression coverage in:
  - `rust/src/cli/oneshot.rs`
- Real installed-binary repro with a slow local server:
  1. `browser start --session spikedog --headless about:blank`
  2. `goto --session spikedog http://127.0.0.1:8881`
  3. while navigation was in flight:
     - `browser status --session spikedog`
     - `title --session spikedog`
     - `snapshot --session spikedog`
- Result:
  - `browser status` returned:
    - `state="incomplete"`
    - `reasons=["daemon_unresponsive"]`
  - both `title` and `snapshot` now returned the same structured busy story
  - they no longer collapsed to bare `daemon_transient`
- Interpretation:
  - the first ship unit for the busy project is now proven on the installed binary, not just in focused tests

## CLI dogfooding: forced-stop cleanup gap reconfirmed (2026-06-06)

- Same repro cleanup path:
  - `browser stop --session spikedog` eventually returned:
    - `{"forced":true,"had_daemon":true,"session":"spikedog","stopped":true}`
  - `browser list` for that `OPENPAGE_HOME` then showed zero sessions
- But Chrome for:
  - `/tmp/openpage-dogfood-busy-spike.fZFooj/profiles/spikedog`
  remained alive until manually killed.
- Interpretation:
  - forced stop currently means daemon cleanup succeeded
  - it does not yet mean browser-child cleanup is complete

## CLI dogfooding: `--replace` truthfulness slice (2026-06-06)

- Narrow implementation landed:
  - `rust/src/cli/oneshot.rs`
  - when `browser start --session <name> --replace ...` is used, the CLI now quiet-stops that named session first, then runs the existing start path
- Help/reference wording also now says the narrower truth:
  - `--replace` restarts an existing named session runtime
  - it preserves that session's profile directory unless the caller explicitly changes profile paths

### Healthy-session installed-binary smoke

- Fresh `OPENPAGE_HOME=/tmp/openpage-replace-smoke.U3eRO4`
- Installed binary:
  - `/tmp/openpage-cli-eval/bin/openpage`
- Steps:
  1. `browser start --session replace-dog --headless https://example.com`
  2. `js --session replace-dog "localStorage.setItem('replace-smoke','persisted'); ..."`
  3. `browser status --session replace-dog`
     - before replace:
       - `pid=52592`
       - `port=53157`
  4. `browser start --session replace-dog --replace --headless https://example.com`
  5. `browser status --session replace-dog`
     - after replace:
       - `pid=52857`
       - `port=53543`
  6. `js --session replace-dog "localStorage.getItem('replace-smoke')"`
  7. `title --session replace-dog`
- Result:
  - the second `browser start --replace` no longer returned `already_running=true`
  - it launched a fresh daemon/browser runtime on a new pid/port
  - localStorage still returned `persisted`
  - `title` still returned `Example Domain`
- Interpretation:
  - the first truthful `--replace` ship unit is now real for healthy named sessions
  - this confirms the semantic decision:
    - `--replace` means runtime restart
    - not fresh-state reset

### Busy-session installed-binary smoke

- Fresh `OPENPAGE_HOME=/tmp/openpage-replace-busy.JKEFPr`
- Local slow server on `127.0.0.1:8882` sleeping `40s`
- Steps:
  1. `browser start --session replace-busy --headless about:blank`
  2. `goto --session replace-busy http://127.0.0.1:8882`
  3. while navigation was in flight:
     - `browser status --session replace-busy`
       - returned `state="incomplete"` / `reasons=["daemon_unresponsive"]`
     - `browser start --session replace-busy --replace --headless https://example.com`
- Result:
  - the original in-flight `goto` later exited with:
    - `state="inactive"`
    - message telling the caller the old session was no longer active
  - the replacement start no longer fell through to `already_running=true`
  - but it failed with a Chrome `SingletonLock` / profile-lock startup error
  - `browser list` for that `OPENPAGE_HOME` was already empty
  - the old Chrome process for:
    - `/tmp/openpage-replace-busy.JKEFPr/profiles/replace-busy`
    remained alive until manually killed
- Interpretation:
  - `--replace` is no longer a fake flag
  - but busy-session recovery still depends on the unresolved browser-child cleanup project
  - this narrows the remaining recovery work:
    - replace truthfulness for normal sessions is handled
    - forced-stop/orphan-Chrome cleanup is now the blocking sub-problem for busy-session recovery

## CLI dogfooding: replace-interruption semantics under busy recovery (2026-06-06)

- Fresh `OPENPAGE_HOME=/tmp/openpage-recovery-int.CcZcv2`
- Installed binary:
  - `/tmp/openpage-cli-eval/bin/openpage`
- Local slow server on `127.0.0.1:8883` sleeping `45s`
- Steps:
  1. `browser start --session recover-int --headless about:blank`
  2. `goto --session recover-int http://127.0.0.1:8883`
  3. while navigation was in flight, run:
     - `title --session recover-int`
     - `snapshot --session recover-int`
     - `browser status --session recover-int`
     - `browser logs --session recover-int --tail 20`
  4. then run:
     - `browser start --session recover-int --replace --headless https://example.com`
- Result:
  - before replacement:
    - `browser status` returned `state="incomplete"` / `reasons=["daemon_unresponsive"]`
    - `browser logs` returned the same state plus an empty-log hint
  - after replacement interrupted the old session:
    - `title` returned `state="inactive"`
    - `snapshot` returned `state="inactive"`
    - the original in-flight `goto` returned:
      - `kind="daemon_transient"`
      - `io error: Connection reset by peer (os error 54)`
      - `Retry the same command.`
  - the replacement start still failed on Chrome `SingletonLock` / profile lock because the old browser process remained alive
- Interpretation:
  - there is not yet one coherent interruption/cancellation story for commands displaced by `--replace`
  - read-style follow-up commands already degrade to `inactive`
  - but the original in-flight navigation still leaks through as generic `daemon_transient`
  - this looks like remaining busy/interruption semantic debt, not a new top-level product project

## CLI dogfooding: profile-lock launch failures still produce misleading recovery advice (2026-06-06)

- In the same `recover-int` repro:
  - the replacement launch failed with Chrome `ProcessSingleton` / `SingletonLock` stderr
  - but the shell error stayed:
    - `kind="browser_launch"`
    - fix text telling the user to verify browser-path resolution / executable validity
- Interpretation:
  - this is not a browser-path problem
  - it is a recovery cleanup / orphan-browser problem
  - current `browser_launch` fix text is too coarse for recovery-path failures that come from profile locks after forced-stop cleanup

## CLI dogfooding: normal stop vs forced stop cleanup split (2026-06-06)

- Goal:
  - verify whether browser-child leakage is a general shutdown defect or specifically a forced-stop recovery defect

### A. Normal stop control run

- Fresh `OPENPAGE_HOME=/tmp/openpage-normal-stop.yqbpiL`
- Steps:
  1. `browser start --session normal-stop --headless https://example.com`
  2. `browser status --session normal-stop`
     - daemon pid:
       - `58357`
     - browser pid from process tree:
       - `58359`
  3. `browser stop --session normal-stop`
- Result:
  - returned:
    - `forced=false`
    - `had_daemon=true`
  - follow-up `ps` showed no remaining Chrome process for:
    - `/tmp/openpage-normal-stop.yqbpiL/profiles/normal-stop`
  - follow-up `browser list` was empty

### B. Forced-stop recovery run

- Fresh `OPENPAGE_HOME=/tmp/openpage-forced-stop.adSX9K`
- Local slow server on `127.0.0.1:8884`
- Steps:
  1. `browser start --session forced-stop --headless about:blank`
  2. `goto --session forced-stop http://127.0.0.1:8884`
  3. `browser status --session forced-stop`
     - returned `state="incomplete"` / `reasons=["daemon_unresponsive"]`
  4. `browser stop --session forced-stop`
- Result:
  - returned:
    - `forced=true`
    - `had_daemon=true`
  - the old in-flight `goto` later failed as `inactive`
  - follow-up `browser list` was empty
  - but the Chrome process for:
    - `/tmp/openpage-forced-stop.adSX9K/profiles/forced-stop`
    remained alive until manually killed

- Interpretation:
  - browser-child leakage is not a generic stop bug
  - it is concentrated in the forced cleanup path
  - this sharpens Project 2:
    - the main remaining recovery defect is forced-stop browser-child cleanup, not ordinary graceful shutdown

## CLI dogfooding: startup observation recheck did not produce a new top-level project (2026-06-06)

- Goal:
  - decide whether a transient startup/list inconsistency deserved to outrank batch readability
- Fresh `OPENPAGE_HOME=/tmp/openpage-start-race.bgdrqN`
- Steps:
  1. `browser start --session race-a --headless about:blank`
  2. `browser start --session race-b --headless https://example.com`
  3. immediately check:
     - `browser list`
     - `browser status --session race-a`
     - `browser status --session race-b`
- Result:
  - `browser list` showed both sessions under `healthy`
  - both status calls returned `state="healthy"` and `target_exists=true`
- Interpretation:
  - the earlier one-off startup observation anomaly was not strong enough to become a separate optimization project
  - keep treating startup truthfulness as part of the existing busy/health semantics stream unless a stronger repro appears

## CLI dogfooding: deeper batch readability evidence (2026-06-06)

- Fresh `OPENPAGE_HOME=/tmp/openpage-batch-deep.FogYRh`
- Steps:
  1. `browser start --session batch-dog --headless https://example.com`
  2. run:
     - `batch "title --session batch-dog" "snapshot --session batch-dog" "click definitely-bad --session batch-dog" "url --session batch-dog"`
  3. run again with:
     - `batch --bail ...`
- Result:
  - non-bail output was four raw NDJSON lines:
    - title success
    - large snapshot success payload
    - element-not-found failure
    - url success
  - bail output was the same first three raw NDJSON lines, then stopped
  - neither mode printed:
    - command index
    - original argv/command text
    - an explicit marker for which command triggered `--bail`
- Code path confirms the shell shape:
  - `rust/src/cli/oneshot.rs::run_batch(...)` simply loops commands and prints each command's native JSON result
  - there is no correlation envelope around each line
- Interpretation:
  - this is a real transcript UX problem, not just a stylistic preference
  - but it still sits below the first two projects because runtime correctness/recovery truthfulness issues remain more structural

## CLI dogfooding: code-level boundary for forced-stop cleanup (2026-06-06)

- Goal:
  - confirm whether Project 2 still needs discovery work, or whether the missing piece is already localized in code

### Current ownership from the code

- Graceful stop path:
  - `rust/src/cli/connection.rs::shutdown_daemon(...)`
    - sends `daemon.shutdown`
  - `rust/src/cli/serve.rs::ServeRuntime::dispatch(...)`
    - handles `daemon.shutdown`
    - calls `close_all_webpages()`
  - `rust/src/cli/serve.rs::close_all_webpages()`
    - drains runtime pages
    - calls `state.page.quit()`
  - `rust/src/webpage.rs::WebPage::quit()`
    - calls `self.browser.close()`
- Forced path:
  - `rust/src/cli/connection.rs::shutdown_daemon(...)`
    - after timeout, falls back to `kill_stale_daemon(session)`
  - `rust/src/cli/connection.rs::kill_stale_daemon(...)`
    - only reads the daemon pid sidecar
    - only kills that daemon pid
    - then removes sidecars

### Why this matters

- Browser child pid is already available in runtime/browser objects:
  - `rust/src/browser.rs`
    - stores `browser_pid: Option<u32>`
    - exposes `Browser::browser_pid()`
  - `rust/src/webpage.rs`
    - exposes `WebPage::browser_pid()`
- But daemon-backed CLI session metadata persisted to disk only includes:
  - `port`
  - `pid` (daemon pid)
  - `version`
  - `log`
- There is no daemon-session browser-pid sidecar or equivalent persisted cleanup handle today.
- The only shell-visible `browser_pid` usage I found is in:
  - `rust/src/cli/mod.rs`
    - DP-compat launch output
  - not in the active daemon-backed session shutdown path

### Refined implementation read

- Project 2 no longer needs broad discovery.
- The smallest credible implementation boundary is:
  1. persist browser child pid (or equivalent durable cleanup handle) for daemon-backed sessions
  2. teach forced cleanup to kill that browser child as well as the daemon pid
  3. then update recovery fix text/tests around profile-lock relaunch failures
- Likely touchpoints:
  - `rust/src/cli/serve.rs`
  - `rust/src/cli/connection.rs`
  - maybe narrow tests in `connection.rs`
- Current model check:
  - for the active daemon-backed CLI flow, one browser pid per session is a credible first-pass model
  - evidence:
    - `ServeRuntime::create_webpage(...)` creates one `WebPage` for the session target
    - tab/window workflows operate through that stored `ServeWebPage.page`
    - target switches use `with_target(...)` / `activate_tab(...)`, which clone/repoint the same browser-backed object rather than launching a second browser process
  - this does not prove all future daemon protocol extensions fit the same assumption, but it is sufficient for the current session-backed CLI surface
- Interpretation:
  - this strengthens Project 2 as an execution-ready optimization project, not just a vague recovery concern
