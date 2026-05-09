---
name: detail-bugs
description: Interact with Detail bugs for a repository via the CLI — list and filter bugs, inspect reports, close as resolved or dismissed, and reopen previously closed bugs.
---

# Detail Bugs

The Detail CLI exposes per-repository bugs through four subcommands: `list`, `show`, `close`, and `reopen`. This skill describes that surface so you can pick the right command for whatever the user is trying to do.

## Prerequisites

The Detail CLI must be installed. If it is not available, install it with:
```
curl --proto '=https' --tlsv1.2 -LsSf https://cli.detail.dev | sh
```

The user must be authenticated. Assume that the user is authed and run commands directly. If a command fails with an authentication error, run `detail auth login` and guide the user through the process.

## Repository Inference

The Detail CLI infers the repository from the git remote; if the user specifies a different repo, pass it explicitly to the CLI commands.

## Subcommands

### `detail bugs list [REPO]`

Lists bugs for the inferred or specified repository.

- `--status pending|resolved|dismissed` — default `pending`; comma-separate or repeat the flag to combine (e.g. `--status resolved,dismissed`).
- `--vulns` — only security vulnerabilities.
- `--introduced-by <authors>` — filter by authors (comma-separated or repeated).
- `--scan-id <wr_…>` — limit to a specific scan. Workflow IDs come from `detail scans list`.
- `--since` / `--until` — accept a duration (`1d`, `24h`), an ISO date (`YYYY-MM-DD`), or an RFC3339 timestamp.
- `--all` — auto-paginate across all matching bugs.
- `--limit <1-100>` (default 50), `--page <N>` (default 1).
- `--format table|json` (default `table`).

### `detail bugs show <BUG_ID>`

Shows the full report for a single bug. Reports often include a suggested fix.

- `--format table|json` — use `json` when parsing rather than displaying.

### `detail bugs close <BUG_ID>`

Marks a bug as resolved or dismissed. The CLI prompts for `--state` interactively in a TTY; pass it explicitly when invoking non-interactively.

- `--state resolved|dismissed`.
- `--dismissal-reason not-a-bug|wont-fix|duplicate|other` — required when state is `dismissed`.
- `--notes "..."` — optional free-form context.
- `--format table|json`.

### `detail bugs reopen <BUG_ID>`

Flips a previously resolved or dismissed bug back to `pending`. Takes only the bug ID — useful when a fix is reverted or a dismissal is overturned.
