use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};
use dialoguer::{Input, Select};

use crate::api::client::ApiClient;
use crate::api::types::{
    dismissal_reason_label, review_state_label, Bug, BugDismissalReason, BugId, BugReviewState,
    IntroducedIn, RepoId,
};
use crate::output::{output_list, SectionRenderer};
use crate::utils::datetime::format_datetime;
use crate::utils::pagination::page_to_offset;
use crate::utils::repos::resolve_repo_id;

/// Return only bugs where `isSecurityVulnerability` is `true`.
fn filter_vulns_only(bugs: &[Bug]) -> Vec<Bug> {
    bugs.iter()
        .filter(|b| b.is_security_vulnerability == Some(true))
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

fn paginate_items<T: Clone>(items: &[T], page: u32, limit: u32) -> Vec<T> {
    let offset = usize::try_from(page_to_offset(page, limit)).unwrap_or(0);
    items
        .iter()
        .skip(offset)
        .take(usize::try_from(limit).unwrap_or(0))
        .cloned()
        .collect()
}

/// Format blame/attribution info for display, e.g. "PR #42 (abc1234) on 2024-12-23 by alice".
fn format_introduced_in(intro: &IntroducedIn) -> String {
    let commit = intro.sha.get(..7).unwrap_or(&intro.sha);
    let ref_label = intro
        .pr_number
        .map_or_else(|| commit.to_string(), |pr| format!("PR #{pr} ({commit})"));
    let date_part = format!(" on {}", intro.date);
    let author_part = intro
        .author
        .as_deref()
        .map(|a| format!(" by {a}"))
        .unwrap_or_default();
    format!("{ref_label}{date_part}{author_part}")
}

#[derive(Subcommand)]
pub enum BugCommands {
    /// List bugs for a given repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli)
        repo: String,

        /// Status filter
        #[arg(long, value_enum, default_value = "pending")]
        status: BugReviewState,

        /// Only show security vulnerabilities
        #[arg(long)]
        vulns: bool,

        /// Only show bugs introduced by these authors (comma-separated or repeat flag)
        #[arg(long, value_delimiter = ',')]
        introduced_by: Vec<String>,

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

/// Page size used when scanning all bugs for client-side vulnerability filtering.
const BUG_PAGE_SIZE: u32 = 100;

/// Fetch every bug for a repo/status by paginating through all pages.
async fn fetch_all_bugs(
    client: &ApiClient,
    repo_id: &RepoId,
    status: BugReviewState,
) -> Result<Vec<Bug>> {
    let mut all_bugs = Vec::new();
    let mut offset = 0;

    loop {
        let response = client
            .list_bugs(repo_id, status, BUG_PAGE_SIZE, offset)
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

pub async fn handle(command: &BugCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        BugCommands::List {
            repo,
            status,
            vulns,
            introduced_by,
            limit,
            page,
            format,
        } => {
            // Resolve owner/repo or repo to internal repo ID
            let resolved_repo_id = resolve_repo_id(&client, repo)
                .await
                .context("Failed to resolve repository identifier")?;

            if *vulns || !introduced_by.is_empty() {
                let all_bugs = fetch_all_bugs(&client, &resolved_repo_id, *status).await?;
                let mut filtered = all_bugs;
                if *vulns {
                    filtered = filter_vulns_only(&filtered);
                }
                if !introduced_by.is_empty() {
                    let pre_filter = filtered;
                    filtered = filter_by_introduced_by(&pre_filter, introduced_by);
                    if filtered.is_empty() {
                        let known = collect_authors(&pre_filter);
                        let hint = if known.is_empty() {
                            "No bugs matched --introduced-by. None of the current bugs have author information.".to_string()
                        } else {
                            format!(
                                "No bugs matched --introduced-by. Known authors: {}",
                                known.join(", ")
                            )
                        };
                        Term::stdout().write_line(&hint)?;
                        return Ok(());
                    }
                }
                let total = filtered.len();
                let page_items = paginate_items(&filtered, *page, *limit);
                output_list(&page_items, total, *page, *limit, format)
            } else {
                let offset = page_to_offset(*page, *limit);
                let bugs = client
                    .list_bugs(&resolved_repo_id, *status, *limit, offset)
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

        BugCommands::Show { bug_id } => {
            let bug_id: BugId = bug_id
                .as_str()
                .try_into()
                .context("Invalid bug ID format (expected bug_...)")?;
            let bug = client
                .get_bug(&bug_id)
                .await
                .context("Failed to fetch bug details")?;

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
            SectionRenderer::new()
                .key_value("", &pairs)
                .markdown("", &bug.summary)
                .print()
        }

        BugCommands::Close {
            bug_id,
            state,
            dismissal_reason,
            notes,
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

            client
                .update_bug_close(&bug_id, state, dismissal_reason, notes.as_deref())
                .await
                .context("Failed to close bug")?;

            Term::stdout()
                .write_line(&format!(
                    "{}",
                    style(format!("✓ Bug closed as: {}", review_state_label(&state))).green()
                ))
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
                "isSecurityVulnerability": true
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "Off-by-one", "summary": "...",
                "createdAt": 2_000_000, "repoId": "repo_1",
                "isSecurityVulnerability": false
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_3", "title": "Missing null check", "summary": "...",
                "createdAt": 3_000_000, "repoId": "repo_1"
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
                "isSecurityVulnerability": false
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_b", "title": "B", "summary": "...",
                "createdAt": 2, "repoId": "repo_1"
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
                "createdAt": 1, "repoId": "repo_1",
                "introducedIn": { "sha": "abc1234", "date": "2024-01-01", "author": "alice" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "Bug by Bob", "summary": "...",
                "createdAt": 2, "repoId": "repo_1",
                "introducedIn": { "sha": "def5678", "date": "2024-01-02", "author": "bob" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_3", "title": "Bug no author", "summary": "...",
                "createdAt": 3, "repoId": "repo_1",
                "introducedIn": { "sha": "ghi9012", "date": "2024-01-03" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_4", "title": "Bug no introduced_in", "summary": "...",
                "createdAt": 4, "repoId": "repo_1"
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
                "createdAt": 1, "repoId": "repo_1",
                "introducedIn": { "sha": "abc1234", "date": "2024-01-01" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "No introduced_in", "summary": "...",
                "createdAt": 2, "repoId": "repo_1"
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
                "createdAt": 1, "repoId": "repo_1",
                "introducedIn": { "sha": "aaa", "date": "2024-01-01", "author": "alice" }
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "bug_2", "title": "B", "summary": "...",
                "createdAt": 2, "repoId": "repo_1",
                "introducedIn": { "sha": "bbb", "date": "2024-01-02", "author": "alice" }
            }))
            .unwrap(),
        ];
        assert_eq!(collect_authors(&bugs), vec!["alice"]);
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
}
