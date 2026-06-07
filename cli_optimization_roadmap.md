# OpenPage CLI Optimization Roadmap

Date: 2026-06-06

This is the short final readout from local dogfooding of the installed CLI at:

- `/tmp/openpage-cli-eval/bin/openpage`

It is intentionally narrower than `notes.md` and `task_plan.md`.
It captures the current ranking, the evidence that held up under local use, and the smallest next slices worth shipping.

Related files:

- `cli_optimization_projects.md`
- `cli_optimization_checklists.md`
- `cli_optimization_issue_cards.md`
- `cli_busy_state_matrix.md`
- `cli_surface_map.md`
- `cli_cut_points.md`
- `cli_pr_sequence.md`
- `cli_workpacks.md`
- `cli_exec_summary.md`
- `cli_active_backlog.md`

## Current-tree revalidation

Fresh local runs on 2026-06-06 reaffirmed that this ranking matches the current worktree behavior, not just earlier exploratory notes.

What held up:

- healthy multi-session startup is no longer the top problem:
  - in a fresh `OPENPAGE_HOME`, `dog-a` and `dog-b` both started cleanly
  - they came up on different ports (`52498`, `52503`)
  - `browser list` and `doctor --quick` both reported them healthy
- healthy-session `--replace` is now real:
  - `dog-a` restarted from port `52498` to `52778`
  - same-session `localStorage` survived the restart
- busy-session recovery still has a real gap:
  - `browser status` suggested logs and `--replace`
  - `browser logs --tail 20` returned an empty log
  - `browser stop --session busy-dog` returned `forced=true`
  - `browser list` then showed no `busy-dog` session
  - Chrome for `/tmp/openpage-long-dog.eccLK7/profiles/busy-dog` was still alive in `ps`
  - follow-up `browser start --session busy-dog --replace ...` failed on `SingletonLock`
- interrupted in-flight requests still do not converge on one state story:
  - after `--replace` displaced an in-flight `goto`, `browser status` reported `daemon_unresponsive`
  - `snapshot` fell into an `unknown target` path
  - the original `goto` later exited as `io error: daemon port not found`
  - after plain `browser stop`, the original in-flight `goto` again exited as `io error: daemon port not found`
- forced-stop recovery still loses observability too early:
  - after `browser stop --session force-probe` returned `forced=true`, the orphan Chrome process still remained alive
  - `browser list` no longer showed the session
  - `doctor --quick --fix` did not surface or repair the remaining problem
  - the next visible user failure was a profile-lock `browser_launch` error with browser-path-oriented advice
- repeatability check strengthened the ranking:
  - two repeated busy + `--replace` runs both reproduced the same profile-lock recovery failure
  - two repeated busy + `browser stop` runs both converged the interrupted `goto` to structured `inactive`
  - earlier low-level `io error: daemon port not found` leakage still matters as evidence that interruption semantics are not fully settled, but it now looks timing-sensitive rather than the dominant repeated outcome
- `batch` readability is still weak:
  - mixed-result `--bail` output still emits only raw per-command JSON lines
  - there is still no command index, argv echo, or explicit stopping-command marker
- broader interaction flow did not dislodge the ranking:
  - `snapshot` / `fill` / `click` / link navigation all worked in a local interactive page flow
  - one minor follow-up issue did show up:
    - clicking a non-navigating JS button still returned a `navigation_token`
    - `wait-for-navigation` on that token then timed out
  - that is worth cleanup later, but it is not bigger than the top three projects
- additional top-level workflows also did not dislodge the ranking:
  - `tab new/list/switch`, `frame list/switch`, `history list/go`, and `storage get` all worked in local repros
  - the remaining friction there looks local rather than structural:
    - `storage get` argument shape is not very discoverable on first use
    - `history go` still expects the usual navigation follow-up (`wait-for-navigation`)
    - `frame switch` already supports `main` / `root` / `page`, but that reset path is easy to miss if users guess
  - those are worth later CLI polish, but they do not currently outrank the first three projects

## Final ranking

### 1. Busy-session control plane and interruption semantics

This is the highest-value project.

Why it matters:

- a long in-flight navigation can monopolize the daemon request lane
- session-backed commands then become inconsistent:
  - `browser status` / `browser list` / `doctor` report an incomplete or unresponsive session
  - other commands can still leak generic transport failure stories
- this is central, not command-local:
  - `rust/src/cli/oneshot.rs` has 202 `rpc_webpage(...)` call sites sharing the same request path

Already validated:

- the first truthfulness slice is landed
- installed-binary repros confirmed `title` / `snapshot` now return structured busy state instead of bare `daemon_transient`

Next slice:

- keep working in `rust/src/cli/connection.rs`
- remap displaced in-flight existing-session requests so they converge on the same structured inactive/interrupted story instead of leaking low-level transport or sidecar errors

## 2. Forced-stop cleanup and recovery truthfulness

This is the second project because recovery guidance is still not equally real in all paths.

Why it matters:

- `browser start --replace` is now truthful for healthy named sessions
- busy-session recovery still breaks on forced-stop cleanup:
  - daemon/session state can disappear
  - Chrome for that profile can still survive
  - replacement launch can then fail on profile lock
- the CLI can also lose observability too early:
  - once forced cleanup removes sidecars, inventory and `doctor --quick --fix` no longer describe the remaining orphan-browser problem
- current fix text still over-relies on logs and `--replace` even when those paths are not sufficient
- this is highly repeatable in local use:
  - repeated busy + `--replace` runs kept reproducing the same `SingletonLock` recovery failure

Already validated:

- healthy-session `--replace` now performs a real restart and preserves the named-session profile
- normal `browser stop` shuts down cleanly
- forced cleanup is the specific remaining defect

Next slice:

- persist a browser-child cleanup handle for daemon-backed sessions
- extend the forced path so it kills the browser child as well as the daemon pid
- then tighten recovery fix text to match the now-real path

## 3. Batch readability and composition UX

This is a real issue, but below the first two.

Why it matters:

- `batch` works, but mixed-result runs are awkward to read
- output is still raw per-command NDJSON
- humans must infer which line belongs to which command
- `--bail` stops the stream without an explicit stopping-command marker

Latest local repro:

- `openpage batch --bail "browser start --session batch-now --headless https://example.com" "title --session batch-now" "title --session missing-now" "browser stop --session batch-now"`
- output contained only three raw JSON lines
- there was no command index, argv echo, or explicit "stopped here" marker

Next slice:

- keep the current execution semantics
- add a small batch envelope per emitted line:
  - command index
  - original argv or command text
  - underlying result/error payload
- make the `--bail` stopping line explicit

## Not top-level right now

- broad `doctor --quick --fix` expansion:
  - current behavior is narrow by documented design
- startup-port policy as a separate project:
  - important evidence came from it earlier, but the later local runs did not dislodge the three projects above
- non-navigating click `navigation_token` truthfulness:
  - real local repro showed this is awkward
  - but it is a narrower interaction-contract cleanup item, not a broader control-plane or recovery project
- tab/frame/history/storage discoverability polish:
  - local repros found a few small help/follow-up discoverability rough edges
  - but the flows themselves worked, so this is not a larger execution or recovery project today
- a large daemon scheduling rewrite:
  - still possible later, but the next value is in smaller shared truthfulness and cleanup slices first

## Lower-priority bucket

If a fourth optimization stream is needed after the top three, the best current candidate is:

- command discoverability and follow-up guidance polish

Why it is real:

- `click` help does not explain that the returned `navigation_token` may be useful only when a click actually navigates
- `history go` help does not mention the usual `wait-for-navigation` follow-up shape
- `frame switch` help does not reveal that `main` / `root` / `page` reset back to the top document
- `storage get` works, but its argument shape is not immediately obvious on first use

Why it is still below the line:

- the underlying commands worked in local dogfooding
- the pain is mostly discoverability and follow-up ergonomics, not runtime correctness or recovery failure

## Current code boundaries

- Project 2 is tightly localized today:
  - `rust/src/cli/connection.rs` forced cleanup still funnels through `kill_stale_daemon(session)`
  - that path reads only the daemon pid sidecar, kills only that pid, then removes sidecars
  - browser child pid already exists in runtime objects, but it is not persisted into the forced cleanup path
- Project 3 is also tightly localized today:
  - `rust/src/cli/oneshot.rs::run_batch(...)` still loops commands and prints each native result/error line directly
  - the first readability fix can stay local to batch output shaping without changing daemon protocol or command execution order

## Recommended order

1. Busy-session displaced-request semantics
2. Forced-stop browser-child cleanup
3. Batch output shaping
