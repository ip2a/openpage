# OpenPage CLI Exec Summary

Date: 2026-06-06

This is the shortest current summary of the local dogfooding result.

Installed CLI used:

- `/tmp/openpage-cli-eval/bin/openpage`

## Final ranking

1. Busy-session control plane / interruption semantics
2. Forced-stop cleanup / recovery truthfulness
3. Batch readability
4. Command discoverability / follow-up guidance polish

## Why this ranking is stable

- repeated local busy-session runs consistently showed that command behavior during interruption is still the highest-breadth problem
- repeated local busy + `--replace` runs consistently reproduced the same profile-lock recovery failure
- mixed-result `batch --bail` runs are still hard to read from stdout alone
- broader workflows like tab/frame/history/storage worked, so they did not dislodge the top three

## If work starts now

1. First PR:
   - make busy/displaced requests converge on one structured state story
   - first cut: `rust/src/cli/connection.rs`
2. Second PR:
   - make forced shutdown kill the browser child as well as the daemon
   - first cut: `rust/src/cli/connection.rs` + daemon-side persisted cleanup handle
3. Third PR:
   - align recovery guidance with the now-real cleanup behavior
4. Fourth PR:
   - add batch command correlation fields and explicit bail stop markers
5. Fifth PR:
   - polish help/follow-up guidance for click/history/frame/storage

## Companion docs

- `cli_optimization_roadmap.md`
- `cli_busy_state_matrix.md`
- `cli_surface_map.md`
- `cli_cut_points.md`
- `cli_pr_sequence.md`
- `cli_workpacks.md`
