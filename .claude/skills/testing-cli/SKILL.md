---
name: testing-cli
description: Test the Detail CLI locally — run lint/tests, verify CLI behavior end-to-end, and diagnose flaky/parallel-test failures. Use when verifying CLI changes or investigating CI failures.
---

# Testing the Detail CLI

## Core commands
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test               # full suite (lib + integration)
cargo xtask check        # vendored artifacts (openapi.json, docs/HELP.md)
cargo run -q -- <args>   # run the CLI directly, e.g. `cargo run -q -- completions`
```

## Investigating CI failures
- `gh run list --limit 20` and `gh run view <run_id> --log-failed` show recent runs and failure logs.
- A test can pass in PR CI but fail on the push to main — suspect flakiness/races, not the diff itself.

## Flaky tests / env-var races
- Rust runs tests in parallel threads, and env vars (`std::env::set_var`) are process-global. Tests that mutate env vars (e.g. `SHELL`) can race and fail intermittently with errors like `called unwrap_err() on an Ok value`.
- Fix pattern: serialize via a `static LOCK: Mutex<()>` and a helper that locks, sets the var, runs the closure, and restores the original value (see `with_shell_var` in `src/commands/completions.rs` tests).
- To prove a flake exists and that a fix works, stress-run with a control:
```bash
# control on a pre-fix worktree, then on the fixed branch
git worktree add /tmp/cli-main origin/main
fails=0; for i in $(seq 1 200); do cargo test --lib <module> >/dev/null 2>&1 || fails=$((fails+1)); done; echo $fails
```
  Expect failures on the control and 0 on the fix. If the control never fails, the result is inconclusive — increase iterations or thread count.

## Merging
- Add the `mergequeue` label when a PR is ready; Aviator handles the merge.

## Devin Secrets Needed
- None for build/lint/test. API-touching commands (auth, bugs, scans) need a `dtl_live_`/`dtl_test_` API token or PKCE login.
