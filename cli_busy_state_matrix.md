# OpenPage CLI Busy-State Matrix

Date: 2026-06-06

This file captures current-tree command behavior around busy sessions and interruption.

Installed CLI used for the runs below:

- `/tmp/openpage-cli-eval/bin/openpage`

It is narrower than `notes.md` and exists to sharpen the boundary between:

- Project 1: busy-session control plane / interruption semantics
- Project 2: forced-stop cleanup / recovery truthfulness

## Scenario A: Busy, not interrupted yet

Setup:

- fresh `OPENPAGE_HOME`
- `browser start --session mx --headless about:blank`
- start `goto --session mx http://127.0.0.1:8899`
- local server delays response by 45 seconds

Observed during the busy window:

- `browser status --session mx`
  - `state="incomplete"`
  - `reasons=["daemon_unresponsive"]`
- `browser list`
  - session moved under `incomplete[]`
  - `reasons=["daemon_unresponsive"]`
- `browser logs --session mx --tail 20`
  - log exists
  - `content=""`
  - still points users back to logs + restart advice
- `title --session mx`
  - remained running for more than 60 seconds in local repro
- `snapshot --session mx`
  - remained running for more than 60 seconds in local repro

Interpretation:

- inventory/status surfaces already converge on the busy story
- logs are still not useful evidence in this state
- ordinary session commands can still be starved badly enough to look hung from the shell

This strengthens Project 1.

## Scenario B: Busy + stop

Setup:

- same delayed local server pattern
- start `goto`
- during the busy window run `browser stop --session recur`

Repeated local outcome:

- before stop:
  - `browser status` reported `daemon_unresponsive`
- stop result in repeated 40-second-delay runs:
  - `{"forced": false, "stopped": true}`
- follow-up shell state:
  - interrupted `goto` converged to structured `inactive`
  - `title` also converged to structured `inactive`

Important nuance:

- in longer / more pathological runs, stop can still fall through to `forced=true`
- so this scenario splits into:
  - ordinary busy stop that still shuts down gracefully
  - deeper busy stop that needs forced cleanup and becomes Project 2

Interpretation:

- Project 1 is not only about low-level leakage
- the central question is whether all interrupted commands converge on one consistent state story

## Scenario C: Busy + replace

Setup:

- same delayed local server pattern
- start `goto`
- during the busy window run:
  - `browser start --session recur --replace --headless https://example.com`

Repeated local outcome:

- before replace:
  - `browser status` reported `daemon_unresponsive`
- replace result:
  - `browser_launch`
  - stderr shows `SingletonLock`
- follow-up shell state:
  - interrupted `goto` converged to structured `inactive`
  - `title` also converged to structured `inactive`

Interpretation:

- the repeated blocker here is no longer interruption semantics alone
- the stable failure is recovery truthfulness:
  - `--replace` is the advertised recovery move
  - preserved-profile restart still collides with orphan Chrome/profile lock

This strengthens Project 2.

## Boundary read

### Project 1 owns:

- busy/unresponsive classification during the in-flight window
- whether ordinary commands hang too long or converge promptly
- whether interrupted commands tell one coherent session-state story

### Project 2 owns:

- whether forced cleanup actually clears the browser child
- whether the recovery path preserves observability until cleanup is complete
- whether `--replace` can succeed after a forced or broken busy-session stop

## Current takeaway

The ranking still holds:

1. busy-session control plane / interruption semantics
2. forced-stop cleanup / recovery truthfulness
3. batch readability

What this matrix adds is sharper ownership:

- Project 1 explains the busy window and shell-level state convergence
- Project 2 explains why the recommended recovery action still fails after that window
