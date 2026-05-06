# Command-Line Help for `detail`

This document contains the help content for the `detail` command-line program.

**Command Overview:**

* [`detail`↴](#detail)
* [`detail auth`↴](#detail-auth)
* [`detail auth login`↴](#detail-auth-login)
* [`detail auth logout`↴](#detail-auth-logout)
* [`detail auth status`↴](#detail-auth-status)
* [`detail bugs`↴](#detail-bugs)
* [`detail bugs list`↴](#detail-bugs-list)
* [`detail bugs show`↴](#detail-bugs-show)
* [`detail bugs close`↴](#detail-bugs-close)
* [`detail bugs reopen`↴](#detail-bugs-reopen)
* [`detail completions`↴](#detail-completions)
* [`detail rules`↴](#detail-rules)
* [`detail rules create`↴](#detail-rules-create)
* [`detail rules propose`↴](#detail-rules-propose)
* [`detail rules requests`↴](#detail-rules-requests)
* [`detail rules requests list`↴](#detail-rules-requests-list)
* [`detail rules requests show`↴](#detail-rules-requests-show)
* [`detail rules list`↴](#detail-rules-list)
* [`detail rules show`↴](#detail-rules-show)
* [`detail rules pull`↴](#detail-rules-pull)
* [`detail satisfying-sort`↴](#detail-satisfying-sort)
* [`detail repos`↴](#detail-repos)
* [`detail repos list`↴](#detail-repos-list)
* [`detail scans`↴](#detail-scans)
* [`detail scans list`↴](#detail-scans-list)
* [`detail skill`↴](#detail-skill)
* [`detail skill rules`↴](#detail-skill-rules)
* [`detail update`↴](#detail-update)
* [`detail version`↴](#detail-version)

## `detail`

Detail CLI - Manage bugs from your terminal

Common workflow:
  1. List pending bugs:   detail bugs list <owner/repo>
  2. View a bug report:   detail bugs show <bug_id>
  3. Fix the bug
  4. Close the bug:       detail bugs close <bug_id>

**Usage:** `detail <COMMAND>`

###### **Subcommands:**

* `auth` — Manage login credentials
* `bugs` — List, show, and close bugs
* `completions` — Install shell completions (auto-detects your shell)
* `rules` — Create and inspect rules
* `satisfying-sort` — Run a fun animation. Humans only
* `repos` — Manage repos tracked with Detail
* `scans` — List and inspect scans
* `skill` — Install Detail skills (default: detail-bugs)
* `update` — Update immediately (auto-update also runs in the background)
* `version` — Show version information



## `detail auth`

Manage login credentials

**Usage:** `detail auth <COMMAND>`

###### **Subcommands:**

* `login` — Login with your Detail account
* `logout` — Logout and remove stored credentials
* `status` — Show current authentication status



## `detail auth login`

Login with your Detail account

**Usage:** `detail auth login [OPTIONS]`

###### **Options:**

* `--token <TOKEN>` — API token (`dtl_live_...`) — skips the browser flow



## `detail auth logout`

Logout and remove stored credentials

**Usage:** `detail auth logout`



## `detail auth status`

Show current authentication status

**Usage:** `detail auth status`



## `detail bugs`

List, show, and close bugs

**Usage:** `detail bugs <COMMAND>`

###### **Subcommands:**

* `list` — List bugs for a given repository
* `show` — Show the report for a bug
* `close` — Close a bug as resolved or dismissed
* `reopen` — Reopen a previously resolved or dismissed bug — flips it back to pending. Useful when a "fix" PR is reverted or a "won't fix" decision is overturned



## `detail bugs list`

List bugs for a given repository

**Usage:** `detail bugs list [OPTIONS] [REPO]`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli). If omitted, inferred from the git remote (origin)

###### **Options:**

* `--status <STATUS>` — Status filter — repeat the flag or comma-separate values to combine (e.g. `--status pending,resolved`). Default: pending

  Default value: `pending`

  Possible values: `pending`, `resolved`, `dismissed`

* `--vulns` — Only show security vulnerabilities
* `--introduced-by <INTRODUCED_BY>` — Only show bugs introduced by these authors (comma-separated or repeat flag)
* `--scan-id <SCAN_ID>` — Filter bugs to a specific scan by workflow request ID
* `--since <SINCE>` — Only show bugs created at or after this point. Accepts a duration (e.g. 1d, 24h, 30m) interpreted as "now minus this", an ISO date (YYYY-MM-DD), or an RFC3339 timestamp
* `--until <UNTIL>` — Only show bugs created at or before this point. Same forms as --since
* `--all` — Auto-paginate: fetch every matching bug instead of a single page
* `--limit <LIMIT>` — Maximum number of results per page

  Default value: `50`
* `--page <PAGE>` — Page number (starts at 1)

  Default value: `1`
* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail bugs show`

Show the report for a bug

**Usage:** `detail bugs show [OPTIONS] <BUG_ID>`

###### **Arguments:**

* `<BUG_ID>` — Bug ID

###### **Options:**

* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail bugs close`

Close a bug as resolved or dismissed

**Usage:** `detail bugs close [OPTIONS] <BUG_ID>`

###### **Arguments:**

* `<BUG_ID>` — Bug ID

###### **Options:**

* `--state <STATE>` — Close state (prompted interactively if omitted in a TTY)

  Possible values: `pending`, `resolved`, `dismissed`

* `--dismissal-reason <DISMISSAL_REASON>` — Dismissal reason (required if state is dismissed)

  Possible values: `not-a-bug`, `wont-fix`, `duplicate`, `other`

* `--notes <NOTES>` — Additional notes
* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail bugs reopen`

Reopen a previously resolved or dismissed bug — flips it back to pending. Useful when a "fix" PR is reverted or a "won't fix" decision is overturned

**Usage:** `detail bugs reopen <BUG_ID>`

###### **Arguments:**

* `<BUG_ID>` — Bug ID



## `detail completions`

Install shell completions (auto-detects your shell)

**Usage:** `detail completions`



## `detail rules`

Create and inspect rules

**Usage:** `detail rules <COMMAND>`

###### **Subcommands:**

* `create` — Submit a rule creation request for a repository
* `propose` — Ask Detail to propose rules for a repository
* `requests` — Check the status of rule creation requests
* `list` — List completed rules for a repository
* `show` — Show a rule's details and content
* `pull` — Pull a rule's generated files locally



## `detail rules create`

Submit a rule creation request for a repository

**Usage:** `detail rules create [OPTIONS] [REPO]`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name. If omitted, inferred from the git remote (origin)

###### **Options:**

* `--description <DESCRIPTION>` — Description of the rule to create
* `--bug-ids <BUG_IDS>` — Bug IDs to use as context (comma-separated or repeat flag)
* `--commit-shas <COMMIT_SHAS>` — Commit SHAs to examine for patterns (comma-separated or repeat flag)



## `detail rules propose`

Ask Detail to propose rules for a repository

**Usage:** `detail rules propose [REPO]`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name. If omitted, inferred from the git remote (origin)



## `detail rules requests`

Check the status of rule creation requests

**Usage:** `detail rules requests <COMMAND>`

###### **Subcommands:**

* `list` — List rule creation requests for a repository
* `show` — Show details and status of a rule creation request



## `detail rules requests list`

List rule creation requests for a repository

**Usage:** `detail rules requests list [OPTIONS] [REPO]`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name. If omitted, inferred from the git remote (origin)

###### **Options:**

* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail rules requests show`

Show details and status of a rule creation request

**Usage:** `detail rules requests show <REQUEST_ID>`

###### **Arguments:**

* `<REQUEST_ID>` — Rule creation request ID (rcr_...)



## `detail rules list`

List completed rules for a repository

**Usage:** `detail rules list [OPTIONS] [REPO]`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name. If omitted, inferred from the git remote (origin)

###### **Options:**

* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail rules show`

Show a rule's details and content

**Usage:** `detail rules show <RULE_ID>`

###### **Arguments:**

* `<RULE_ID>` — Rule ID (rule_...)



## `detail rules pull`

Pull a rule's generated files locally

**Usage:** `detail rules pull [OPTIONS] <RULE_ID>`

###### **Arguments:**

* `<RULE_ID>` — Rule ID (rule_...)

###### **Options:**

* `--output <OUTPUT>` — Skill directory to write detail-rules/ into (defaults to .claude/skills/)



## `detail satisfying-sort`

Run a fun animation. Humans only

**Usage:** `detail satisfying-sort`



## `detail repos`

Manage repos tracked with Detail

**Usage:** `detail repos <COMMAND>`

###### **Subcommands:**

* `list` — List all repositories you have access to



## `detail repos list`

List all repositories you have access to

**Usage:** `detail repos list [OPTIONS]`

###### **Options:**

* `--limit <LIMIT>` — Maximum number of results per page

  Default value: `50`
* `--page <PAGE>` — Page number (starts at 1)

  Default value: `1`
* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail scans`

List and inspect scans

**Usage:** `detail scans <COMMAND>`

###### **Subcommands:**

* `list` — List recent scans for a repository



## `detail scans list`

List recent scans for a repository

**Usage:** `detail scans list [OPTIONS] [REPO]`

###### **Arguments:**

* `<REPO>` — Repository in owner/repo format or just repo name. If omitted, inferred from the git remote (origin)

###### **Options:**

* `--status <STATUS>` — Filter by workflow status (e.g. failed scans in the last day)

  Possible values: `in-progress`, `complete`, `failed`, `dlq`

* `--scan-type <SCAN_TYPE>` — Filter by scan type

  Possible values: `default`, `recent-changes`

* `--since <SINCE>` — Only show scans created at or after this point. Accepts a duration (e.g. 1d, 24h, 30m), an ISO date (YYYY-MM-DD), or an RFC3339 timestamp
* `--until <UNTIL>` — Only show scans created at or before this point. Same forms as --since
* `--limit <LIMIT>` — Maximum number of results per page

  Default value: `50`
* `--page <PAGE>` — Page number (starts at 1)

  Default value: `1`
* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail skill`

Install Detail skills (default: detail-bugs)

**Usage:** `detail skill [COMMAND]`

###### **Subcommands:**

* `rules` — Install the detail-create-rules skill



## `detail skill rules`

Install the detail-create-rules skill

**Usage:** `detail skill rules`



## `detail update`

Update immediately (auto-update also runs in the background)

**Usage:** `detail update`



## `detail version`

Show version information

**Usage:** `detail version`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
