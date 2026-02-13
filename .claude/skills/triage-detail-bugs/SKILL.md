---
name: triage-detail-bugs
description: Triage Detail bugs for a repository. Lists pending bugs, shows details for each, and helps resolve or dismiss them one by one.
argument-hint: [owner/repo]
allowed-tools: Bash
---

# Triage Detail Bugs

Walk through pending bugs for the repository `$ARGUMENTS` and help the user triage each one.

## Prerequisites

The Detail CLI must be installed. If it is not available, install it with:

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/usedetail/cli/releases/latest/download/detail-cli-installer.sh | sh
```

The user must be authenticated. Ask the user for their Detail API token, then run:

```
detail auth login --token <TOKEN>
```

## Step 1: List pending bugs

Run `detail bugs list $ARGUMENTS` to fetch all pending bugs.

Present the list to the user and ask which bug they want to look at first, or offer to go through them in order.

## Step 2: Show bug details

For each bug the user wants to review, run `detail bugs show <bug_id>` to display the full report.

## Step 3: Implement a fix

After reviewing the bug report, implement a fix for the bug in the codebase.

## Step 4: Resolve or dismiss

After implementing the fix (or if the bug is not valid), ask the user whether to resolve or dismiss:

- **Resolve**: `detail bugs review <bug_id> --state resolved`
- **Dismiss**: `detail bugs review <bug_id> --state dismissed --dismissal-reason <reason>`
  - Valid dismissal reasons: `not-a-bug`, `wont-fix`, `duplicate`, `other`
  - Optionally add `--notes "..."` if the user provides context

After the action completes, move to the next bug and repeat from Step 2.
