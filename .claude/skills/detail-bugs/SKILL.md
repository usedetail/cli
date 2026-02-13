---
name: detail-bugs
description: Fetch Detail bugs for a repository. List pending bugs, show details for each, fix the bugs, and mark them as resolved or dismissed.
argument-hint: [owner/repo]
allowed-tools: Bash
---

# Fetch Detail Bugs

Get bugs from Detail for the repository `$ARGUMENTS` (or the current repository
if not provided) and help the user triage them.

## Prerequisites

The Detail CLI must be installed. If it is not available, install it with:
```
curl --proto '=https' --tlsv1.2 \
  -LsSf https://github.com/usedetail/cli/releases/latest/download/detail-cli-installer.sh \
  | sh
```

The user must be authenticated. Ask the user for their Detail API token, then
run:
```
detail auth login --token <TOKEN>
```

## Step 1: List pending bugs

Run `detail bugs list $ARGUMENTS` to fetch all pending bugs.

Present the list to the user and ask which bug they want to look at first, or
offer to go through them in order.

## Step 2: Show bug details

For each bug the user wants to review, run `detail bugs show <bug_id>` to
display the full report.

## Step 3: Implement a fix

After reviewing the bug report, implement a fix for the bug in the codebase.
Many bugs will have a suggested fix as part of the report, which you may want
to confirm with the user before implementing.

## Step 4: Resolve or dismiss

After implementing the fix, mark the bug as resolved. If we are not going to
fix the bug, mark it as dismissed.

- **Resolve**: `detail bugs close <bug_id> --state resolved`
- **Dismiss**: `detail bugs close <bug_id> --state dismissed --dismissal-reason <reason>`
  - Valid dismiss reasons: `not-a-bug`, `wont-fix`, `duplicate`, `other`
  - Add `--notes "..."` if the user provides context.

Then move to the next bug and repeat.
