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
* [`detail rules`↴](#detail-rules)
* [`detail rules create`↴](#detail-rules-create)
* [`detail rules list`↴](#detail-rules-list)
* [`detail rules show`↴](#detail-rules-show)
* [`detail satisfying-sort`↴](#detail-satisfying-sort)
* [`detail repos`↴](#detail-repos)
* [`detail repos list`↴](#detail-repos-list)
* [`detail skill`↴](#detail-skill)
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
* `rules` — Create and inspect rules
* `satisfying-sort` — Run a fun animation. Humans only
* `repos` — Manage repos tracked with Detail
* `skill` — Install the detail-bugs skill
* `update` — Update Immediately (auto-update also runs in the background)
* `version` — Show version information



## `detail auth`

Manage login credentials

**Usage:** `detail auth <COMMAND>`

###### **Subcommands:**

* `login` — Login with an API token
* `logout` — Logout and remove stored credentials
* `status` — Show current authentication status



## `detail auth login`

Login with an API token

**Usage:** `detail auth login [OPTIONS]`

###### **Options:**

* `--token <TOKEN>` — API token (dtl_live_...)



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



## `detail bugs list`

List bugs for a given repository

**Usage:** `detail bugs list [OPTIONS] <REPO>`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli)

###### **Options:**

* `--status <STATUS>` — Status filter

  Default value: `pending`

  Possible values: `pending`, `resolved`, `dismissed`

* `--vulns` — Only show security vulnerabilities
* `--introduced-by <INTRODUCED_BY>` — Only show bugs introduced by these authors (comma-separated or repeat flag)
* `--limit <LIMIT>` — Maximum number of results per page

  Default value: `50`
* `--page <PAGE>` — Page number (starts at 1)

  Default value: `1`
* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail bugs show`

Show the report for a bug

**Usage:** `detail bugs show <BUG_ID>`

###### **Arguments:**

* `<BUG_ID>` — Bug ID



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



## `detail rules`

Create and inspect rules

**Usage:** `detail rules <COMMAND>`

###### **Subcommands:**

* `create` — Start an async rule creation job for a repository
* `list` — List rule creation requests for a repository
* `show` — Show the details or status of a rule creation request



## `detail rules create`

Start an async rule creation job for a repository

**Usage:** `detail rules create [OPTIONS] <REPO>`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name

###### **Options:**

* `--description <DESCRIPTION>` — Description of the rule to create
* `--bug-id <BUG_IDS>` — Bug ID to use as context (repeatable)
* `--commit-sha <COMMIT_SHAS>` — Commit SHA to examine for patterns (repeatable)



## `detail rules list`

List rule creation requests for a repository

**Usage:** `detail rules list [OPTIONS] <REPO>`

###### **Arguments:**

* `<REPO>` — Repository by owner/repo (e.g., usedetail/cli) or repo name

###### **Options:**

* `--format <FORMAT>` — Output format

  Default value: `table`

  Possible values: `table`, `json`




## `detail rules show`

Show the details or status of a rule creation request

**Usage:** `detail rules show <REQUEST_ID>`

###### **Arguments:**

* `<REQUEST_ID>` — Rule creation request ID (rcr_...)



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




## `detail skill`

Install the detail-bugs skill

**Usage:** `detail skill`



## `detail update`

Update Immediately (auto-update also runs in the background)

**Usage:** `detail update`



## `detail version`

Show version information

**Usage:** `detail version`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
