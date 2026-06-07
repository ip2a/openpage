# Session Management

How OpenPage daemon-backed sessions work today, and how to use them without reintroducing old protocol confusion.

## The model

OpenPage's active CLI surface is session-based, but the execution truth is still the same TCP daemon path underneath.

That means:

- `browser start --session foo` is an explicit bootstrap command
- `goto --session foo` may bootstrap a missing session before navigating
- `click --session foo`
- `snapshot --session foo`

are all higher-level wrappers over the same daemon-backed control path.

There is no separate legacy attach mode you should prefer over this.

The current shell rule is intentionally narrow:

- `browser start` and `goto` can create or revive the named daemon-backed session
- other `--session` commands such as `title`, `html`, `snapshot`, `click`, `js`, and `screenshot` require an already active session
- if the session is missing, those follow-up commands fail fast instead of silently starting a fresh browser

## Start, inspect, stop

Use these commands as the stable session lifecycle:

```bash
openpage browser start --session review --headless https://example.com
openpage browser status --session review
openpage browser logs --session review --tail 20
openpage browser list
openpage browser stop --session review
openpage browser stop --all
```

If you want to prove the guardrail is working, this should now fail:

```bash
OPENPAGE_HOME=/tmp/openpage-missing cargo run --manifest-path rust/Cargo.toml --bin openpage -- title --session missing
```

Expected result:

- JSON error with `error.kind="browser_operation"`
- the same top-level JSON error also carries `error.session="missing"` for known session-control failures
- the same top-level JSON error also carries `error.fix` when the CLI knows the next control-plane step
- for known session-control failures, the same top-level JSON error also carries `error.state`
  and stable `error.reasons` when applicable
- for known transient daemon failures, the same top-level JSON error also carries
  `error.retryable=true` plus a stable `error.suggested_action` such as `retry_same_command`
- direct CLI daemon errors now preserve these structured fields more directly across the
  daemon-response round trip instead of depending only on message-text reconstruction
- when the CLI has no such recovery hint, `error.fix` is omitted instead of emitted as `null`
- message tells you to run `openpage browser start --session missing`
- no new daemon sidecars should be created for that missing session

`browser list` currently returns:

- `summary` — counts for `healthy`, `incompatible`, `incomplete`, `cleaned`, and `total`
- `sessions` — ready daemon-backed sessions, each with:
  - `state="healthy"` when its daemon version matches the current CLI
  - `state="incompatible"` plus `reasons=["version_mismatch"]` when it is still live but version-stale
  - `fix` when the session needs an explicit stop/restart action
- `incomplete` — live daemons with incomplete sidecars, each with `state="incomplete"` and stable `reasons`
- `cleaned` — stale sidecars cleaned during the scan, each with `state="cleaned"`

`browser logs --session review --tail 20` returns the same session metadata and shell-level
`state` as `browser status`, plus tailed daemon log content when a persisted `.log` file exists.
If it returns `exists=false` and `content=null`, that means the session currently has no persisted
stderr log file to read. If the session is incomplete, the stable `reasons[]` also stay visible in
the `browser logs` payload. If `OPENPAGE_CONTENT_BOUNDARIES=1` or
`OPENPAGE_MAX_OUTPUT_CHARS` is set, the `content` field also goes through the same boundary /
truncate filters as page text payloads.
When the session needs an explicit control-plane action, the same payload also carries a `fix`
string, so callers do not need to reverse-engineer the next step from `state` and `reasons`.

`browser stop --all` is the current shell-level way to close every active daemon-backed session
discovered in the same `OPENPAGE_HOME`.

`browser status --session review` now also returns:

- `state="healthy"` for a ready daemon-backed session
- `state="incompatible"` plus `reasons=["version_mismatch"]` for a ready but version-stale daemon session
- `state="incomplete"` plus `incomplete` / `reasons` fields when the daemon is alive but not ready
- `state="inactive"` when no live daemon currently matches that session name
- `fix` when the session needs a concrete next action such as stop/restart, doctor cleanup, or start

If the state is `incompatible`, follow-up `--session` commands fail fast until you stop and
restart that session with the current CLI. This keeps the active TCP daemon protocol surface
stable instead of letting a live old-version daemon keep serving follow-up commands.

## Default session vs named sessions

Most commands default to:

```text
--session default
```

That is fine for quick local testing.

For any non-trivial workflow, prefer explicit names:

```bash
openpage browser start --session auth --headless https://app.example.com/login
openpage browser start --session scrape --headless https://docs.example.com
```

## What a session isolates

In the current OpenPage CLI design, a session is the unit that owns:

- the running daemon-backed browser runtime
- current page state
- active tab / active frame tracking
- cookies
- localStorage / sessionStorage
- session-scoped browser commands such as snapshot, click, download, tabs, and frames

Operationally, each session also has sidecar state under:

```text
$OPENPAGE_HOME/daemon
```

including the `.port`, `.pid`, `.version`, and `.log` files used for runtime discovery and audit.

## Active tab and frame state

One reason the old direct per-command attach model was fragile is that it had to reconstruct page context repeatedly.

The current daemon-backed session model keeps the active tab and active frame inside the daemon runtime.

Practical implication:

- if you switch tabs or frames, that becomes the current session context
- after switching, re-snapshot before continuing ref-based interaction

Example:

```bash
openpage click-for-new-tab @e3 --session review
openpage snapshot --session review
openpage tab switch t1 --session review
openpage snapshot --session review
```

## Authentication and state reuse

OpenPage's current CLI does not expose a separate `state save/load` command pair on the active user surface.

Today, the supported ways to preserve or reconstruct useful state are:

1. keep the named session alive for the duration of the task
2. use `cookies get/set/delete/clear` deliberately
3. use `storage get/set --scope local|session` deliberately
4. start with a deliberate `--user-data-dir` only when you explicitly want profile reuse

That means the safest default for agent tasks is:

- use a semantic named session
- keep it alive while the task runs
- stop it explicitly when done

## Multi-session patterns

### Parallel isolation

```bash
openpage browser start --session public --headless https://example.com
openpage browser start --session admin --headless https://admin.example.com
```

This is the right way to compare different auth states or roles without mixing cookies and tab state.

### Reproducible review session

```bash
openpage browser start --session review --replace --headless https://example.com
openpage snapshot --session review
openpage browser stop --session review
```

Use `--replace` when you want to restart the runtime for a known session name while keeping that session's default profile continuity.

## Best practices

### 1. Use semantic session names

Prefer:

- `review`
- `auth`
- `docs-scrape`
- `checkout-debug`

Avoid generic names like `s1` or `test2` unless the scope is truly throwaway.

### 2. Audit with `browser list` and `doctor`

If session behavior looks inconsistent, run:

```bash
openpage browser list
openpage doctor --quick
```

This separates session/sidecar issues from browser executable/config issues.
`doctor --quick` now also returns a machine-readable `inventory` block, so the same command gives
you both:

- check-oriented health/fix output
- daemon-related `checks[]` entries that now also carry machine-readable `state` / `reasons` when
  the check is about a concrete daemon session or incomplete sidecar set
- those daemon-related `checks[]` entries also carry `session` directly for concrete session checks
- the current daemon runtime truth via `summary` / `sessions` / `incomplete` / `cleaned`
- even when no daemon directory exists yet, `inventory` now stays present as an empty object
  instead of returning `null`
- the same stable incomplete-session `reasons[]` taxonomy used by `browser status` and `browser logs`

If the problem is only shell-level residue from the removed one-shot path, an incompatible daemon session, or an incomplete unready daemon session, run:

```bash
openpage doctor --quick --fix
```

That cleanup is intentionally narrow:

- it removes legacy session JSON files under `OPENPAGE_HOME/sessions`
- it cleans stale dead daemon sidecars discovered during the audit
- it stops incompatible daemon sessions whose `.version` does not match the current CLI
- it stops incomplete daemon sessions only when they are **unready**
- it does **not** touch healthy active TCP daemon sessions that already match the current CLI version

### 3. Stop sessions you no longer need

```bash
openpage browser stop --session review
openpage browser stop --all
```

Long-lived sessions are useful, but abandoned sessions create noisy runtime state and make audits harder.

Current outer-shell behavior:

- `browser stop` first asks the daemon to shut down cleanly
- if the daemon is still alive but unresponsive, the CLI falls back to forced cleanup so the session does not linger as the active truth
- `browser stop --all` reuses that same per-session shutdown path for every active session currently discovered by `browser list`

### 4. Re-snapshot after context shifts

Tab changes, frame changes, and navigations are context shifts. Treat them as ref invalidation points.

## Practical rule

Use named sessions as the stable unit of work, but remember that the true execution path is still the same TCP daemon underneath.
