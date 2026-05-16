use std::convert::TryInto;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};
use dialoguer::{Input, Select};

use crate::api::client::ApiClient;
use crate::api::types::{
    dismissal_reason_label, format_introduced_in, format_linked_issue, review_state_label, Bug,
    BugDismissalReason, BugId, BugReviewState, ListPublicBugsWorkflowRequestId, RepoId,
};
use crate::output::{output_list, SectionRenderer};
use crate::utils::datetime::{format_datetime, parse_time_spec};
use crate::utils::git::resolve_repo_arg;
use crate::utils::pagination::page_to_offset;
use crate::utils::repos::resolve_repo_id;

/// Return only bugs where `isSecurityVulnerability` is `true`.
fn filter_vulns_only(bugs: &[Bug]) -> Vec<Bug> {
    bugs.iter()
        .filter(|b| b.is_security_vulnerability == Some(true))
        .cloned()
        .collect()
}

/// Resolve a `--since` / `--until` flag value to epoch millis, flattening
/// `parse_time_spec`'s error into the top-level message so users see the
/// accepted-form list without needing `RUST_LOG`-style chain expansion.
fn resolve_time_flag(
    name: &str,
    value: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<i64>> {
    value.map_or(Ok(None), |s| {
        parse_time_spec(s, now)
            .map(|dt| Some(dt.timestamp_millis()))
            .map_err(|e| anyhow::anyhow!("invalid {name} value: {e}"))
    })
}

/// Return only bugs whose `createdAt` falls within the given inclusive bounds.
fn filter_by_time_range(bugs: &[Bug], since_ms: Option<i64>, until_ms: Option<i64>) -> Vec<Bug> {
    bugs.iter()
        .filter(|b| {
            since_ms.is_none_or(|s| b.created_at >= s) && until_ms.is_none_or(|u| b.created_at <= u)
        })
        .cloned()
        .collect()
}

/// Return only bugs whose `introducedIn.author` case-insensitively matches one of `authors`.
fn filter_by_introduced_by(bugs: &[Bug], authors: &[String]) -> Vec<Bug> {
    bugs.iter()
        .filter(|b| {
            b.introduced_in
                .as_ref()
                .and_then(|i| i.author.as_deref())
                .is_some_and(|a| authors.iter().any(|name| a.eq_ignore_ascii_case(name)))
        })
        .cloned()
        .collect()
}

/// Collect the sorted, deduplicated set of authors present in `bugs`.
fn collect_authors(bugs: &[Bug]) -> Vec<&str> {
    let mut authors: Vec<&str> = bugs
        .iter()
        .filter_map(|b| b.introduced_in.as_ref()?.author.as_deref())
        .collect();
    authors.sort_unstable();
    authors.dedup();
    authors
}

/// Build the hint message shown when `--introduced-by` filtering yields no results.
///
/// `pre_filter` is the set of bugs remaining after `--vulns` but before
/// `--introduced-by`. When it is empty, the real reason for the empty result is
/// the prior filter (or no bugs at all) — not missing author metadata — so the
/// hint must not mention authors.
fn empty_filter_hint(pre_filter: &[Bug], vulns: bool) -> String {
    if pre_filter.is_empty() {
        if vulns {
            "No security vulnerabilities found with the current filters.".to_string()
        } else {
            "No bugs found with the current filters.".to_string()
        }
    } else {
        let known = collect_authors(pre_filter);
        if known.is_empty() {
            "No bugs matched --introduced-by. None of the current bugs have author information."
                .to_string()
        } else {
            format!(
                "No bugs matched --introduced-by. Known authors: {}",
                known.join(", ")
            )
        }
    }
}

fn paginate_items<T: Clone>(items: &[T], page: u32, limit: u32) -> Vec<T> {
    let offset = usize::try_from(page_to_offset(page, limit)).unwrap_or(0);
    items
        .iter()
        .skip(offset)
        .take(usize::try_from(limit).unwrap_or(0))
        .cloned()
        .collect()
}

#[derive(Subcommand)]
pub enum BugCommands {
    /// List bugs for a given repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli).
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,

        /// Status filter — repeat the flag or comma-separate values to
        /// combine (e.g. `--status pending,resolved`). Default: pending.
        #[arg(long, value_enum, value_delimiter = ',', default_value = "pending")]
        status: Vec<BugReviewState>,

        /// Only show security vulnerabilities
        #[arg(long)]
        vulns: bool,

        /// Only show bugs introduced by these authors (comma-separated or repeat flag)
        #[arg(long, value_delimiter = ',')]
        introduced_by: Vec<String>,

        /// Filter bugs to a specific scan by workflow request ID
        #[arg(long)]
        scan_id: Option<String>,

        /// Only show bugs created at or after this point.
        /// Accepts a duration (e.g. 1d, 24h, 30m) interpreted as "now minus
        /// this", an ISO date (YYYY-MM-DD), or an RFC3339 timestamp.
        #[arg(long)]
        since: Option<String>,

        /// Only show bugs created at or before this point. Same forms as --since.
        #[arg(long)]
        until: Option<String>,

        /// Auto-paginate: fetch every matching bug instead of a single page.
        #[arg(long, conflicts_with_all = ["page", "limit"])]
        all: bool,

        /// Maximum number of results per page
        #[arg(long, default_value = "50", value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,

        /// Page number (starts at 1)
        #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..))]
        page: u32,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Show the report for a bug
    Show {
        /// Bug ID
        bug_id: String,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Close a bug as resolved or dismissed
    Close {
        /// Bug ID
        bug_id: String,

        /// Close state (prompted interactively if omitted in a TTY)
        #[arg(long, value_enum)]
        state: Option<BugReviewState>,

        /// Dismissal reason (required if state is dismissed)
        #[arg(long, value_enum)]
        dismissal_reason: Option<BugDismissalReason>,

        /// Additional notes
        #[arg(long)]
        notes: Option<String>,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// Reopen a previously resolved or dismissed bug — flips it back to
    /// pending. Useful when a "fix" PR is reverted or a "won't fix"
    /// decision is overturned.
    Reopen {
        /// Bug ID
        bug_id: String,
    },
}

// ── Interactive prompt helpers ──────────────────────────────────────

/// Prompt for close state (Resolved / Dismissed) via arrow-key selection.
fn prompt_close_state() -> Result<BugReviewState> {
    let items = ["Resolved", "Dismissed"];
    let selection = Select::new()
        .with_prompt("Close state")
        .items(items)
        .default(0)
        .interact()
        .context("Failed to read close state selection")?;
    match selection {
        0 => Ok(BugReviewState::Resolved),
        _ => Ok(BugReviewState::Dismissed),
    }
}

/// Prompt for dismissal reason via arrow-key selection.
fn prompt_dismissal_reason() -> Result<BugDismissalReason> {
    let items = ["Not a Bug", "Won't Fix", "Duplicate", "Other"];
    let selection = Select::new()
        .with_prompt("Dismissal reason")
        .items(items)
        .default(0)
        .interact()
        .context("Failed to read dismissal reason selection")?;
    match selection {
        0 => Ok(BugDismissalReason::NotABug),
        1 => Ok(BugDismissalReason::WontFix),
        2 => Ok(BugDismissalReason::Duplicate),
        _ => Ok(BugDismissalReason::Other),
    }
}

/// Prompt for optional notes via text input.
fn prompt_notes() -> Result<Option<String>> {
    let input: String = Input::new()
        .with_prompt("Notes (optional)")
        .allow_empty(true)
        .interact_text()
        .context("Failed to read notes input")?;
    if input.is_empty() {
        Ok(None)
    } else {
        Ok(Some(input))
    }
}

/// Validate and resolve the close-command flags that can be checked without
/// interactive prompts.  Returns `Ok(CloseParams)` when all flags are
/// present, or `Err` when a flag combination is invalid.  Returns `Ok(None)`
/// for the `state/dismissal_reason` fields that still need interactive input.
///
/// Rules:
/// - `--state pending` is always rejected.
/// - When `is_interactive` is false, `--state` is required.
/// - When state is `Dismissed` and `is_interactive` is false,
///   `--dismissal-reason` is required.
/// - When state is not `Dismissed`, any supplied `--dismissal-reason` is
///   passed through (the API will ignore it).
fn validate_close_flags(
    state: Option<BugReviewState>,
    dismissal_reason: Option<BugDismissalReason>,
    notes: Option<String>,
    is_interactive: bool,
) -> Result<(
    Option<BugReviewState>,
    Option<BugDismissalReason>,
    Option<String>,
)> {
    // Reject --state pending
    if matches!(state, Some(BugReviewState::Pending)) {
        bail!("'pending' is not a valid close state. Use 'resolved' or 'dismissed'.");
    }

    // Non-interactive: --state is required
    let state = match state {
        Some(s) => Some(s),
        None if is_interactive => None, // will prompt
        None => bail!(
            "--state is required in non-interactive mode. Use --state resolved or --state dismissed."
        ),
    };

    // Non-interactive + Dismissed: --dismissal-reason is required
    let dismissal_reason = if matches!(state, Some(BugReviewState::Dismissed)) {
        match dismissal_reason {
            Some(r) => Some(r),
            None if is_interactive => None, // will prompt
            None => bail!(
                "--dismissal-reason is required when state is 'dismissed' in non-interactive mode."
            ),
        }
    } else {
        dismissal_reason
    };

    Ok((state, dismissal_reason, notes))
}

/// Render a single bug as the human-readable `bugs show` view.
fn render_bug_show(bug: &Bug) -> Result<()> {
    let mut pairs: Vec<(&str, String)> = vec![
        ("ID", bug.id.to_string()),
        ("Title", bug.title.clone()),
        ("File", bug.file_path.as_deref().unwrap_or("-").to_string()),
        ("Created", format_datetime(bug.created_at)),
        (
            "Security",
            bug.is_security_vulnerability
                .map_or("-", |v| if v { "Yes" } else { "No" })
                .to_string(),
        ),
    ];
    if let Some(intro) = &bug.introduced_in {
        pairs.push(("Introduced", format_introduced_in(intro)));
    }
    if let Some(review) = &bug.review {
        pairs.push(("Close", review_state_label(&review.state).to_string()));
        pairs.push(("Close Date", format_datetime(review.created_at)));
        if let Some(reason) = &review.dismissal_reason {
            pairs.push(("Dismissal", dismissal_reason_label(reason).to_string()));
        }
        if let Some(notes) = &review.notes {
            pairs.push(("Notes", notes.clone()));
        }
    }
    for issue in &bug.linked_issues {
        pairs.push(("Issue", format_linked_issue(issue)));
    }
    SectionRenderer::new()
        .key_value("", &pairs)
        .markdown("", &bug.summary)
        .print()
}

/// Page size used when scanning all bugs for client-side vulnerability filtering.
const BUG_PAGE_SIZE: u32 = 100;

/// Fetch every bug for a repo/status by paginating through all pages.
async fn fetch_all_bugs(
    client: &ApiClient,
    repo_id: &RepoId,
    status: BugReviewState,
    scan_id: Option<&ListPublicBugsWorkflowRequestId>,
) -> Result<Vec<Bug>> {
    let mut all_bugs = Vec::new();
    let mut offset = 0;

    loop {
        let response = client
            .list_bugs(repo_id, status, BUG_PAGE_SIZE, offset, scan_id)
            .await
            .context("Failed to fetch bugs from repository")?;

        let total = usize::try_from(response.total.max(0)).unwrap_or(0);
        let page_len = response.bugs.len();
        all_bugs.extend(response.bugs);

        if page_len == 0 || (usize::try_from(offset).unwrap_or(0) + page_len) >= total {
            break;
        }
        offset += u32::try_from(page_len).unwrap_or(u32::MAX);
    }

    Ok(all_bugs)
}

/// Dedupe a slice of `BugReviewState` while preserving first-seen order.
/// Used to normalize repeated `--status` inputs (e.g. `pending,pending`) so
/// the multi-status helper doesn't fan out duplicate API calls and inflate
/// totals.
fn dedupe_statuses(statuses: &[BugReviewState]) -> Vec<BugReviewState> {
    let mut out: Vec<BugReviewState> = Vec::with_capacity(statuses.len());
    for s in statuses {
        if !out.contains(s) {
            out.push(*s);
        }
    }
    out
}

/// Fetch every bug for each of `statuses`, concatenated in the order given.
/// The bugs API only accepts a single status per request, so multi-status
/// queries fan out into one paginated call per status. Repeated statuses
/// are deduped first so `--status pending,pending` doesn't double-count.
async fn fetch_all_bugs_multi_status(
    client: &ApiClient,
    repo_id: &RepoId,
    statuses: &[BugReviewState],
    scan_id: Option<&ListPublicBugsWorkflowRequestId>,
) -> Result<Vec<Bug>> {
    let mut combined = Vec::new();
    for status in dedupe_statuses(statuses) {
        let bugs = fetch_all_bugs(client, repo_id, status, scan_id).await?;
        combined.extend(bugs);
    }
    Ok(combined)
}

/// Fetch a single page of bugs across multiple `statuses`, concatenated in
/// order. The caller's `limit` is distributed evenly across statuses so the
/// merged result never exceeds `limit` items, preserving the page-size
/// contract. Each status gets its own proportional offset derived from
/// `page` and its share of the limit.
///
/// Unlike `fetch_all_bugs_multi_status` this does NOT exhaust every page —
/// it issues one bounded request per status and merges the results, keeping
/// multi-status queries fast even on repos with thousands of bugs.
async fn fetch_page_multi_status(
    client: &ApiClient,
    repo_id: &RepoId,
    statuses: &[BugReviewState],
    limit: u32,
    page: u32,
    scan_id: Option<&ListPublicBugsWorkflowRequestId>,
) -> Result<(Vec<Bug>, usize)> {
    let unique = dedupe_statuses(statuses);
    let n = u32::try_from(unique.len()).unwrap_or(1).max(1);
    let per_status_limit = limit / n;
    let remainder = limit % n;

    let mut combined = Vec::new();
    let mut total: usize = 0;
    for (i, status) in unique.into_iter().enumerate() {
        let idx = u32::try_from(i).unwrap_or(0);
        let sl = per_status_limit + u32::from(idx < remainder);
        let offset = page_to_offset(page, sl);
        let response = client
            .list_bugs(repo_id, status, sl, offset, scan_id)
            .await
            .context("Failed to fetch bugs from repository")?;
        total += usize::try_from(response.total.max(0)).unwrap_or(0);
        combined.extend(response.bugs);
    }
    Ok((combined, total))
}

pub async fn handle(command: &BugCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        BugCommands::List {
            repo,
            status,
            vulns,
            introduced_by,
            scan_id,
            since,
            until,
            all,
            limit,
            page,
            format,
        } => {
            // Resolve owner/repo or repo to internal repo ID
            let repo = resolve_repo_arg(repo.as_deref())?;
            let resolved_repo_id = resolve_repo_id(&client, &repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let scan_id: Option<ListPublicBugsWorkflowRequestId> = scan_id
                .as_deref()
                .map(TryInto::try_into)
                .transpose()
                .context("Invalid scan ID format (expected wr_...)")?;

            // Resolve --since/--until against the same `now` so a relative
            // window like `--since 7d --until 1d` reads as a single half-open
            // interval anchored to the same instant.
            let now = chrono::Utc::now();
            let since_ms = resolve_time_flag("--since", since.as_deref(), now)?;
            let until_ms = resolve_time_flag("--until", until.as_deref(), now)?;

            // The bugs API takes a single status per request. When the
            // user asks for client-side filters (`--all`, `--vulns`,
            // `--introduced-by`, `--since`, `--until`) we must fetch every
            // bug to apply them. Multi-status alone does NOT require a full
            // fetch — we can issue one page-sized request per status.
            let needs_full_fetch = *all
                || *vulns
                || !introduced_by.is_empty()
                || since_ms.is_some()
                || until_ms.is_some();
            let multi_status = status.len() > 1;

            if needs_full_fetch {
                let all_bugs = fetch_all_bugs_multi_status(
                    &client,
                    &resolved_repo_id,
                    status,
                    scan_id.as_ref(),
                )
                .await?;
                let mut filtered = all_bugs;
                if since_ms.is_some() || until_ms.is_some() {
                    filtered = filter_by_time_range(&filtered, since_ms, until_ms);
                }
                if *vulns {
                    filtered = filter_vulns_only(&filtered);
                }
                if !introduced_by.is_empty() {
                    let pre_filter = filtered;
                    filtered = filter_by_introduced_by(&pre_filter, introduced_by);
                    if filtered.is_empty() {
                        if matches!(format, crate::OutputFormat::Table) {
                            let hint = empty_filter_hint(&pre_filter, *vulns);
                            Term::stdout().write_line(&hint)?;
                        }
                        return output_list(&filtered, 0, *page, *limit, format);
                    }
                } else if filtered.is_empty() {
                    // Filters (or `--all` against an empty repo) removed
                    // everything. Print the hint so the user gets context
                    // beyond an empty table.
                    if matches!(format, crate::OutputFormat::Table) {
                        let hint = empty_filter_hint(&filtered, *vulns);
                        Term::stdout().write_line(&hint)?;
                    }
                    return output_list(&filtered, 0, *page, *limit, format);
                }
                let total = filtered.len();
                if *all {
                    // No client-side paging: emit every matching bug as a
                    // single page so JSON consumers and table users alike
                    // see the full result set.
                    let effective_limit = u32::try_from(total.max(1)).unwrap_or(u32::MAX);
                    return output_list(&filtered, total, 1, effective_limit, format);
                }
                let page_items = paginate_items(&filtered, *page, *limit);
                output_list(&page_items, total, *page, *limit, format)
            } else if multi_status {
                // Multiple statuses but no client-side filters: fetch one
                // page per status and merge, avoiding a full exhaust.
                let (bugs, total) = fetch_page_multi_status(
                    &client,
                    &resolved_repo_id,
                    status,
                    *limit,
                    *page,
                    scan_id.as_ref(),
                )
                .await?;
                output_list(&bugs, total, *page, *limit, format)
            } else {
                // Single-status, no other filters: keep the original
                // single-page server fetch — cheaper and lets the API drive
                // pagination.
                let single_status = status.first().copied().unwrap_or(BugReviewState::Pending);
                let offset = page_to_offset(*page, *limit);
                let bugs = client
                    .list_bugs(
                        &resolved_repo_id,
                        single_status,
                        *limit,
                        offset,
                        scan_id.as_ref(),
                    )
                    .await
                    .context("Failed to fetch bugs from repository")?;

                output_list(
                    &bugs.bugs,
                    usize::try_from(bugs.total.max(0)).unwrap_or(0),
                    *page,
                    *limit,
                    format,
                )
            }
        }

        BugCommands::Show { bug_id, format } => {
            let bug_id: BugId = bug_id
                .as_str()
                .try_into()
                .context("Invalid bug ID format (expected bug_...)")?;
            let bug = client
                .get_bug(&bug_id)
                .await
                .context("Failed to fetch bug details")?;

            if matches!(format, crate::OutputFormat::Json) {
                Term::stdout().write_line(&serde_json::to_string_pretty(&bug)?)?;
                return Ok(());
            }
            render_bug_show(&bug)
        }

        BugCommands::Close {
            bug_id,
            state,
            dismissal_reason,
            notes,
            format,
        } => {
            let bug_id: BugId = bug_id
                .as_str()
                .try_into()
                .context("Invalid bug ID format (expected bug_...)")?;
            let is_interactive = Term::stdout().is_term();

            let (state, dismissal_reason, notes) =
                validate_close_flags(*state, *dismissal_reason, notes.clone(), is_interactive)?;

            // Resolve fields that still need interactive prompts
            let state = match state {
                Some(s) => s,
                None => prompt_close_state()?,
            };

            let dismissal_reason = if matches!(state, BugReviewState::Dismissed) {
                match dismissal_reason {
                    Some(r) => Some(r),
                    None => Some(prompt_dismissal_reason()?),
                }
            } else {
                dismissal_reason
            };

            let notes = match notes {
                Some(n) => Some(n),
                None if is_interactive => prompt_notes()?,
                None => None,
            };

            let review = client
                .update_bug_close(&bug_id, state, dismissal_reason, notes.as_deref())
                .await
                .context("Failed to close bug")?;

            if matches!(format, crate::OutputFormat::Json) {
                // Emit only the BugReview JSON — the human-friendly success
                // banner would corrupt the structured output.
                Term::stdout().write_line(&serde_json::to_string_pretty(&review)?)?;
                return Ok(());
            }

            Term::stdout()
                .write_line(&format!(
                    "{}",
                    style(format!("✓ Bug closed as: {}", review_state_label(&state))).green()
                ))
                .ok();
            Ok(())
        }

        BugCommands::Reopen { bug_id } => {
            let bug_id: BugId = bug_id
                .as_str()
                .try_into()
                .context("Invalid bug ID format (expected bug_...)")?;

            // Don't pass notes from the CLI — `create_public_bug_review`
            // replaces the whole review row, so any value (including the
            // implicit None) overwrites whatever notes the existing review
            // already carried. Until the API gains PATCH semantics, the
            // safe shape for `reopen` is a pure state flip.
            client
                .update_bug_close(&bug_id, BugReviewState::Pending, None, None)
                .await
                .context("Failed to reopen bug")?;

            Term::stdout()
                .write_line(&format!("{}", style("✓ Bug reopened (pending)").green()))
                .ok();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_close_flags ─────────────────────────────────────────

    #[test]
    fn close_resolved_non_interactive() {
        let (state, reason, notes) =
            validate_close_flags(Some(BugReviewState::Resolved), None, None, false).unwrap();
        assert!(matches!(state, Some(BugReviewState::Resolved)));
        assert!(reason.is_none());
        assert!(notes.is_none());
    }

    #[test]
    fn close_dismissed_with_reason_non_interactive() {
        let (state, reason, _) = validate_close_flags(
            Some(BugReviewState::Dismissed),
            Some(BugDismissalReason::WontFix),
            None,
            false,
        )
        .unwrap();
        assert!(matches!(state, Some(BugReviewState::Dismissed)));
        assert!(matches!(reason, Some(BugDismissalReason::WontFix)));
    }

    #[test]
    fn close_rejects_pending() {
        let err =
            validate_close_flags(Some(BugReviewState::Pending), None, None, false).unwrap_err();
        assert!(err.to_string().contains("not a valid close state"));
    }

    #[test]
    fn close_rejects_pending_even_interactive() {
        let err =
            validate_close_flags(Some(BugReviewState::Pending), None, None, true).unwrap_err();
        assert!(err.to_string().contains("not a valid close state"));
    }

    #[test]
    fn close_no_state_non_interactive_errors() {
        let err = validate_close_flags(None, None, None, false).unwrap_err();
        assert!(err.to_string().contains("--state is required"));
    }

    #[test]
    fn close_no_state_interactive_defers_to_prompt() {
        let (state, _, _) = validate_close_flags(None, None, None, true).unwrap();
        assert!(state.is_none()); // will be filled by interactive prompt
    }

    #[test]
    fn close_dismissed_no_reason_non_interactive_errors() {
        let err =
            validate_close_flags(Some(BugReviewState::Dismissed), None, None, false).unwrap_err();
        assert!(err.to_string().contains("--dismissal-reason is required"));
    }

    #[test]
    fn close_dismissed_no_reason_interactive_defers() {
        let (state, reason, _) =
            validate_close_flags(Some(BugReviewState::Dismissed), None, None, true).unwrap();
        assert!(matches!(state, Some(BugReviewState::Dismissed)));
        assert!(reason.is_none()); // will be filled by interactive prompt
    }

    #[test]
    fn close_passes_notes_through() {
        let (_, _, notes) = validate_close_flags(
            Some(BugReviewState::Resolved),
            None,
            Some("fixed it".into()),
            false,
        )
        .unwrap();
        assert_eq!(notes.as_deref(), Some("fixed it"));
    }

    #[test]
    fn close_resolved_ignores_dismissal_reason() {
        let (_, reason, _) = validate_close_flags(
            Some(BugReviewState::Resolved),
            Some(BugDismissalReason::Duplicate),
            None,
            false,
        )
        .unwrap();
        // Passed through — the API will ignore it
        assert!(matches!(reason, Some(BugDismissalReason::Duplicate)));
    }

    // ── filter_vulns_only ────────────────────────────────────────

    fn sample_bugs() -> Vec<Bug> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_1", "title": "SQL injection", "summary": "...",
                "createdAt": 1_000_000, "repoId": "repo_1",
                "isSecurityVulnerability": true, "linkedIssues": []
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "Off-by-one", "summary": "...",
                "createdAt": 2_000_000, "repoId": "repo_1",
                "isSecurityVulnerability": false, "linkedIssues": []
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_3", "title": "Missing null check", "summary": "...",
                "createdAt": 3_000_000, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
        ]
    }

    #[test]
    fn vulns_filter_returns_only_security_bugs() {
        let bugs = sample_bugs();
        let filtered = filter_vulns_only(&bugs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "SQL injection");
    }

    #[test]
    fn vulns_filter_on_empty_list() {
        assert!(filter_vulns_only(&[]).is_empty());
    }

    #[test]
    fn vulns_filter_excludes_none_and_false() {
        // Bugs with isSecurityVulnerability: None or false are excluded
        let bugs: Vec<Bug> = vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_a", "title": "A", "summary": "...",
                "createdAt": 1, "repoId": "repo_1",
                "isSecurityVulnerability": false, "linkedIssues": []
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_b", "title": "B", "summary": "...",
                "createdAt": 2, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
        ];
        assert!(filter_vulns_only(&bugs).is_empty());
    }

    // ── filter_by_introduced_by ──────────────────────────────────────

    fn sample_bugs_with_authors() -> Vec<Bug> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_1", "title": "Bug by Alice", "summary": "...",
                "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "abc1234", "date": "2024-01-01", "author": "alice" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "Bug by Bob", "summary": "...",
                "createdAt": 2, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "def5678", "date": "2024-01-02", "author": "bob" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_3", "title": "Bug no author", "summary": "...",
                "createdAt": 3, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "ghi9012", "date": "2024-01-03" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_4", "title": "Bug no introduced_in", "summary": "...",
                "createdAt": 4, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
        ]
    }

    #[test]
    fn introduced_by_matches_single_author() {
        let bugs = sample_bugs_with_authors();
        let filtered = filter_by_introduced_by(&bugs, &["alice".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id.to_string(), "bug_1");
    }

    #[test]
    fn introduced_by_matches_multiple_authors() {
        let bugs = sample_bugs_with_authors();
        let filtered = filter_by_introduced_by(&bugs, &["alice".to_string(), "bob".to_string()]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn introduced_by_is_case_insensitive() {
        let bugs = sample_bugs_with_authors();
        let filtered = filter_by_introduced_by(&bugs, &["ALICE".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id.to_string(), "bug_1");
    }

    #[test]
    fn introduced_by_excludes_bugs_without_author() {
        let bugs = sample_bugs_with_authors();
        // bug_3 has introducedIn but no author; bug_4 has no introducedIn at all
        let filtered = filter_by_introduced_by(&bugs, &["alice".to_string()]);
        assert!(!filtered.iter().any(|b| b.id.to_string() == "bug_3"));
        assert!(!filtered.iter().any(|b| b.id.to_string() == "bug_4"));
    }

    #[test]
    fn introduced_by_empty_list_returns_nothing() {
        let bugs = sample_bugs_with_authors();
        let filtered = filter_by_introduced_by(&bugs, &["nobody".to_string()]);
        assert!(filtered.is_empty());
    }

    // ── dedupe_statuses ──────────────────────────────────────────────

    #[test]
    fn dedupe_statuses_passes_through_distinct_values() {
        let input = [
            BugReviewState::Pending,
            BugReviewState::Resolved,
            BugReviewState::Dismissed,
        ];
        assert_eq!(dedupe_statuses(&input), input.to_vec());
    }

    #[test]
    fn dedupe_statuses_drops_repeats_preserving_first_seen_order() {
        // `--status resolved --status pending --status resolved` →
        // [Resolved, Pending] (resolved deduped, original order kept).
        let input = [
            BugReviewState::Resolved,
            BugReviewState::Pending,
            BugReviewState::Resolved,
        ];
        assert_eq!(
            dedupe_statuses(&input),
            vec![BugReviewState::Resolved, BugReviewState::Pending]
        );
    }

    #[test]
    fn dedupe_statuses_collapses_all_repeats_to_single() {
        // `--status pending,pending,pending` → [Pending]
        let input = [
            BugReviewState::Pending,
            BugReviewState::Pending,
            BugReviewState::Pending,
        ];
        assert_eq!(dedupe_statuses(&input), vec![BugReviewState::Pending]);
    }

    #[test]
    fn dedupe_statuses_handles_empty() {
        assert!(dedupe_statuses(&[]).is_empty());
    }

    // ── filter_by_time_range ─────────────────────────────────────────

    fn time_ranged_bugs() -> Vec<Bug> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_old", "title": "old", "summary": "...",
                "createdAt": 1_000, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_mid", "title": "mid", "summary": "...",
                "createdAt": 2_000, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_new", "title": "new", "summary": "...",
                "createdAt": 3_000, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
        ]
    }

    #[test]
    fn time_range_no_bounds_returns_all() {
        let bugs = time_ranged_bugs();
        assert_eq!(filter_by_time_range(&bugs, None, None).len(), 3);
    }

    #[test]
    fn time_range_since_is_inclusive_lower_bound() {
        let bugs = time_ranged_bugs();
        let filtered = filter_by_time_range(&bugs, Some(2_000), None);
        let ids: Vec<_> = filtered.iter().map(|b| b.id.to_string()).collect();
        assert_eq!(ids, vec!["bug_mid", "bug_new"]);
    }

    #[test]
    fn time_range_until_is_inclusive_upper_bound() {
        let bugs = time_ranged_bugs();
        let filtered = filter_by_time_range(&bugs, None, Some(2_000));
        let ids: Vec<_> = filtered.iter().map(|b| b.id.to_string()).collect();
        assert_eq!(ids, vec!["bug_old", "bug_mid"]);
    }

    #[test]
    fn time_range_both_bounds_clamps_to_window() {
        let bugs = time_ranged_bugs();
        let filtered = filter_by_time_range(&bugs, Some(2_000), Some(2_000));
        let ids: Vec<_> = filtered.iter().map(|b| b.id.to_string()).collect();
        assert_eq!(ids, vec!["bug_mid"]);
    }

    #[test]
    fn time_range_inverted_window_is_empty() {
        // since > until: nothing matches; we don't error, we just return empty.
        let bugs = time_ranged_bugs();
        assert!(filter_by_time_range(&bugs, Some(3_000), Some(1_000)).is_empty());
    }

    // ── collect_authors ──────────────────────────────────────────────

    #[test]
    fn collect_authors_returns_sorted_deduped() {
        let bugs = sample_bugs_with_authors();
        let authors = collect_authors(&bugs);
        assert_eq!(authors, vec!["alice", "bob"]);
    }

    #[test]
    fn collect_authors_empty_when_no_authors() {
        let bugs: Vec<Bug> = vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_1", "title": "No author", "summary": "...",
                "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "abc1234", "date": "2024-01-01" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "No introduced_in", "summary": "...",
                "createdAt": 2, "repoId": "repo_1", "linkedIssues": []
            }))
            .unwrap(),
        ];
        assert!(collect_authors(&bugs).is_empty());
    }

    #[test]
    fn collect_authors_deduplicates() {
        let bugs: Vec<Bug> = vec![
            serde_json::from_value(serde_json::json!({
                "id": "bug_1", "title": "A", "summary": "...",
                "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "aaa", "date": "2024-01-01", "author": "alice" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "B", "summary": "...",
                "createdAt": 2, "repoId": "repo_1", "linkedIssues": [],
                "introducedIn": { "sha": "bbb", "date": "2024-01-02", "author": "alice" }
            }))
            .unwrap(),
        ];
        assert_eq!(collect_authors(&bugs), vec!["alice"]);
    }

    // ── empty_filter_hint ────────────────────────────────────────────

    #[test]
    fn empty_filter_hint_vulns_flag_with_empty_prefilter() {
        // --vulns produced no results: don't mention authors
        let hint = empty_filter_hint(&[], true);
        assert_eq!(
            hint,
            "No security vulnerabilities found with the current filters."
        );
    }

    #[test]
    fn empty_filter_hint_no_vulns_flag_with_empty_prefilter() {
        // Repo had no bugs at all: don't mention authors
        let hint = empty_filter_hint(&[], false);
        assert_eq!(hint, "No bugs found with the current filters.");
    }

    #[test]
    fn empty_filter_hint_prefilter_has_bugs_without_authors() {
        let bugs: Vec<Bug> = vec![serde_json::from_value(serde_json::json!({
            "id": "bug_1", "title": "No author", "summary": "...",
            "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
            "introducedIn": { "sha": "abc1234", "date": "2024-01-01" }
        }))
        .unwrap()];
        let hint = empty_filter_hint(&bugs, true);
        assert_eq!(
            hint,
            "No bugs matched --introduced-by. None of the current bugs have author information."
        );
    }

    #[test]
    fn empty_filter_hint_prefilter_has_known_authors() {
        let bugs = sample_bugs_with_authors();
        let hint = empty_filter_hint(&bugs, false);
        assert_eq!(
            hint,
            "No bugs matched --introduced-by. Known authors: alice, bob"
        );
    }

    // ── paginate_items ───────────────────────────────────────────────

    #[test]
    fn paginate_items_first_page() {
        let items = vec![1, 2, 3, 4, 5];
        let page = paginate_items(&items, 1, 2);
        assert_eq!(page, vec![1, 2]);
    }

    #[test]
    fn paginate_items_second_page() {
        let items = vec![1, 2, 3, 4, 5];
        let page = paginate_items(&items, 2, 2);
        assert_eq!(page, vec![3, 4]);
    }

    #[test]
    fn paginate_items_out_of_range_page_is_empty() {
        let items = vec![1, 2, 3];
        let page = paginate_items(&items, 3, 2);
        assert!(page.is_empty());
    }

    // `format_introduced_in` moved to `crate::api::types`; tests now live
    // alongside the function in `src/api/types.rs`.
}
