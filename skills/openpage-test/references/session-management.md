# Session Management

How OpenPage daemon-backed sessions work today, and how to use them without reintroducing old protocol confusion.

## The model

OpenPage's active CLI surface is session-based, but the execution truth is still the same TCP daemon path underneath.

That means:

- `browser start --session foo`
- `goto --session foo`
- `click --session foo`
- `snapshot --session foo`

are all higher-level wrappers over the same daemon-backed control path.

There is no separate legacy attach mode you should prefer over this.

## Start, inspect, stop

Use these commands as the stable session lifecycle:

```bash
openpage browser start --session review --headless https://example.com
openpage browser status --session review
openpage browser list
openpage browser stop --session review
```

`browser list` currently returns:

- `sessions` — healthy daemon-backed sessions
- `incomplete` — live daemons with incomplete sidecars
- `cleaned` — stale sidecars cleaned during the scan

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

Use `--replace` when you want a clean restart for a known session name.

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
If the only warning is legacy session JSON residue from the old one-shot path, run:

```bash
openpage doctor --quick --fix
```

That cleanup does not touch the active TCP daemon sessions.

### 3. Stop sessions you no longer need

```bash
openpage browser stop --session review
```

Long-lived sessions are useful, but abandoned sessions create noisy runtime state and make audits harder.

### 4. Re-snapshot after context shifts

Tab changes, frame changes, and navigations are context shifts. Treat them as ref invalidation points.

## Practical rule

Use named sessions as the stable unit of work, but remember that the true execution path is still the same TCP daemon underneath.
