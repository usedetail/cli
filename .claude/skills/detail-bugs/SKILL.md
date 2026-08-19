---
name: detail-bugs
description: Interact with Detail bugs for a repository via the CLI — list and filter bugs, inspect reports, close as resolved or dismissed, reopen previously closed bugs, and override a bug's priority.
---

# Detail Bugs

The Detail CLI exposes per-repository bugs through five subcommands: `list`, `show`, `close`, `reopen`, and `prioritize`. This skill describes that surface so you can pick the right command for whatever the user is trying to do.

## Prerequisites

The Detail CLI must be installed. If it is not available, install it with:
```
curl --proto '=https' --tlsv1.2 -LsSf https://cli.detail.dev | sh
```

The user must be authenticated. Assume that the user is authed and run commands directly. If a command fails with an authentication error, run `detail auth login` and guide the user through the process.

## Repository Inference

The Detail CLI infers the repository from the git or jj remote; if the user specifies a different repo, pass it explicitly to the CLI commands.

## Subcommands

### `detail bugs list [REPO]`

Lists bugs for the inferred or specified repository.

- `--status pending|resolved|dismissed` — default `pending`; comma-separate or repeat the flag to combine (e.g. `--status resolved,dismissed`).
- `--vulns` — only security vulnerabilities.
- `--priority p1|p2|p3|none` — only bugs at these priorities; comma-separate or repeat the flag (e.g. `--priority p1,p2`). `none` selects bugs Detail never scored — most bugs found before priority scoring shipped, so prefer `--priority p1,p2,p3` over `--priority p1` when the user asks for "prioritized" bugs. Default: every priority.
- `--sort newest|oldest|priority` — default `newest`. `priority` puts the most severe first and unscored last.
- `--introduced-by <authors>` — filter by authors (comma-separated or repeated).
- `--scan-id <wr_…>` — limit to a specific scan. Workflow IDs come from `detail scans list`.
- `--since` / `--until` — accept a duration (`1d`, `24h`), an ISO date (`YYYY-MM-DD`), or an RFC3339 timestamp.
- `--all` — auto-paginate across all matching bugs.
- `--limit <1-100>` (default 50), `--page <N>` (default 1).
- `--format table|json` (default `table`).

### `detail bugs show <BUG_ID>`

Shows the full report for a single bug. Reports often include a suggested fix.

Also shows `Priority` and, when Detail scored the bug, a `Rationale` explaining why. If someone has since overridden that score, an `Override` line reports what Detail originally assigned and why it was changed.

- `--format table|json` — use `json` when parsing rather than displaying.

### `detail bugs close <BUG_ID>`

Marks a bug as resolved or dismissed. The CLI prompts for `--state` interactively in a TTY; pass it explicitly when invoking non-interactively.

- `--state resolved|dismissed`.
- `--dismissal-reason not-a-bug|wont-fix|duplicate|other` — required when state is `dismissed`.
- `--notes "..."` — optional free-form context.
- `--format table|json`.

### `detail bugs reopen <BUG_ID>`

Flips a previously resolved or dismissed bug back to `pending`. Takes only the bug ID — useful when a fix is reverted or a dismissal is overturned.

### `detail bugs prioritize <BUG_ID>`

Overrides Detail's priority for a bug and records the change on its timeline. The CLI prompts for `--priority` interactively in a TTY; pass it explicitly when invoking non-interactively.

- `--priority p1|p2|p3`.
- `--comment "..."` — why the priority is changing. Worth passing: it is what a later `detail bugs show` reports as the override reason.
- `--format table|json`.

Setting the priority a bug already has is a no-op — the CLI reports "no change" rather than recording a second identical entry.
