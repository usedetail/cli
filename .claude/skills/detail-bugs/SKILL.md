---
name: detail-bugs
description: >-
  Interact with Detail bugs for a repository via the CLI — list and filter bugs, inspect reports, close as resolved or dismissed, reopen previously closed bugs, and override a bug's priority. Also covers security scan findings (`detail vulns`): list, show, close, and reopen.
---

# Detail Bugs

The Detail CLI exposes per-repository bugs through five subcommands: `list`, `show`, `close`, `reopen`, and `prioritize`. Findings from Detail's security scans live separately under `detail vulns`. This skill describes both surfaces so you can pick the right command for whatever the user is trying to do.

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

- `--status open|resolved|dismissed` — default `open`; comma-separate or repeat the flag to combine (e.g. `--status resolved,dismissed`). `pending` is a legacy alias for `open`.
- `--vulns` — only bugs the bug finder flagged as security vulnerabilities. For findings from security scans, use `detail vulns list` instead.
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

Flips a previously resolved or dismissed bug back to `open`. Takes only the bug ID — useful when a fix is reverted or a dismissal is overturned.

### `detail bugs prioritize <BUG_ID>`

Overrides Detail's priority for a bug and records the change on its timeline. The CLI prompts for `--priority` interactively in a TTY; pass it explicitly when invoking non-interactively.

- `--priority p1|p2|p3`.
- `--comment "..."` — why the priority is changing. Worth passing: it is what a later `detail bugs show` reports as the override reason.
- `--format table|json`.

Setting the priority a bug already has is a no-op — the CLI reports "no change" rather than recording a second identical entry.

## Security scan findings (`detail vulns`)

Security scans produce findings ("vulns") that are separate from bugs: they have a severity (`critical`, `high`, `medium`, `low`) instead of a priority, so there is no prioritize for them. They can be closed and reopened like bugs. A finding's state can also change without a review: a merged Detail fix PR or a closed tracker issue sets it, and the latest event wins. Vuln IDs look like `ssf_…`. When the user asks about security scan results or vulnerabilities from a security scan, use these commands rather than `detail bugs list --vulns`.

### `detail vulns list [REPO]`

- `--status open|resolved|dismissed` — one value; default `open`, so closed findings are hidden unless asked for. `pending` is a legacy alias for `open`. To see every state, run it once per state.
- `--severity critical|high|medium|low` — comma-separate or repeat the flag (e.g. `--severity critical,high`). Default: every severity.
- `--sort newest|oldest|severity` — default `newest`. `severity` puts the most severe first.
- `--scan-id <wr_…>` — limit to a specific security scan. Workflow IDs come from `detail vulns scans`.
- `--since` / `--until`, `--all`, `--limit <1-100>` (default 50), `--page <N>`, `--format table|json` — same as `detail bugs list`.

### `detail vulns show <VULN_ID>`

Shows the full finding: severity, state (with dismissal reason, notes, and what set it, when present), file, commit, scan, linked issues, the fix PR if any, and a markdown description of the flaw, how it is reached, and the fix.

- `--format table|json`.

### `detail vulns close <VULN_ID>`

Marks a finding as resolved or dismissed. The CLI prompts for `--state` interactively in a TTY; pass it explicitly when invoking non-interactively.

- `--state resolved|dismissed`.
- `--dismissal-reason not-exploitable|wont-fix|duplicate|other` — required when state is `dismissed`.
- `--notes "..."` — optional free-form context, up to 2000 characters.
- `--format table|json`.

### `detail vulns reopen <VULN_ID>`

Flips a previously resolved or dismissed finding back to `open`. Takes only the vuln ID. The earlier reason and notes do not carry over.

### `detail vulns scans [REPO]`

Lists recent security scans with per-severity finding counts.

- `--status in-progress|complete|failed|dlq`, `--since` / `--until`, `--limit`, `--page`, `--format table|json`.
