use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};
use dialoguer::{Input, Select};

use crate::api::client::ApiClient;
use crate::api::types::{
    dismissal_reason_label, review_state_label, BugDismissalReason, BugId, BugReviewState, Repo,
    RepoId,
};
use crate::output::{output_list, SectionRenderer};
use crate::utils::{format_datetime, page_to_offset};

#[derive(Subcommand)]
pub enum BugCommands {
    /// List bugs for a given repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli)
        repo: String,

        /// Status filter
        #[arg(long, value_enum, default_value = "pending")]
        status: BugReviewState,

        /// Maximum number of results per page
        #[arg(long, default_value = "50")]
        limit: u32,

        /// Page number (starts at 1)
        #[arg(long, default_value = "1")]
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
        .items(&items)
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
        .items(&items)
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
/// for the state/dismissal_reason fields that still need interactive input.
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

/// Page size used when paginating through repos to resolve identifiers.
const REPO_PAGE_SIZE: u32 = 100;

/// Fetch all repos by paginating through the API.
async fn fetch_all_repos(client: &ApiClient) -> Result<Vec<Repo>> {
    let mut all_repos = Vec::new();
    let mut offset = 0;

    loop {
        let repos = client
            .list_repos(REPO_PAGE_SIZE, offset)
            .await
            .context("Failed to fetch repositories while resolving identifier")?;

        let page_size = repos.repos.len();
        all_repos.extend(repos.repos);

        if (page_size as u32) < REPO_PAGE_SIZE {
            break;
        }
        offset += REPO_PAGE_SIZE;
    }

    Ok(all_repos)
}

/// Validate that a slash-containing identifier has exactly one slash with
/// non-empty owner and repo parts.
fn validate_owner_repo_format(identifier: &str) -> Result<()> {
    let parts: Vec<&str> = identifier.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Invalid repository format. Please use owner/repo (e.g., 'usedetail/cli') or just the repo name. Run 'detail repos list' to see your repositories."
        );
    }
    Ok(())
}

/// Given a bare repo name and the full list of accessible repos, return the
/// matching repo ID — or a helpful error when zero or multiple repos match.
fn match_repo_by_name(name: &str, repos: &[Repo]) -> Result<RepoId> {
    let matching: Vec<_> = repos.iter().filter(|r| r.name == name).collect();

    match matching.len() {
        0 => bail!(
            "Repository '{}' not found. Run 'detail repos list' to see your repositories.",
            name
        ),
        1 => Ok(matching[0].id.clone()),
        _ => {
            let repo_list: Vec<String> = matching
                .iter()
                .map(|r| format!("  - {}", r.full_name))
                .collect();
            bail!(
                "Multiple repositories with name '{}' found:\n{}\n\nPlease specify using owner/repo format (e.g., '{}').",
                name,
                repo_list.join("\n"),
                matching[0].full_name
            )
        }
    }
}

/// Resolve owner/repo or repo name to repo ID
async fn resolve_repo_id(client: &ApiClient, repo_identifier: &str) -> Result<RepoId> {
    let repos = fetch_all_repos(client).await?;

    if repo_identifier.contains('/') {
        validate_owner_repo_format(repo_identifier)?;
        repos
            .iter()
            .find(|r| r.full_name == repo_identifier)
            .map(|r| r.id.clone())
            .context(format!(
                "Repository '{}' not found. Make sure you have access to this repository.",
                repo_identifier
            ))
    } else {
        match_repo_by_name(repo_identifier, &repos)
    }
}

pub async fn handle(command: &BugCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        BugCommands::List {
            repo,
            status,
            limit,
            page,
            format,
        } => {
            // Resolve owner/repo or repo to internal repo ID
            let resolved_repo_id = resolve_repo_id(&client, repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let offset = page_to_offset(*page, *limit);

            let bugs = client
                .list_bugs(&resolved_repo_id, *status, *limit, offset)
                .await
                .context("Failed to fetch bugs from repository")?;

            output_list(&bugs.bugs, bugs.total as usize, *page, *limit, format)
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
                        .map(|v| if v { "Yes" } else { "No" })
                        .unwrap_or("-")
                        .to_string(),
                ),
            ];
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

    // ── validate_owner_repo_format ───────────────────────────────────

    #[test]
    fn valid_owner_repo() {
        assert!(validate_owner_repo_format("usedetail/cli").is_ok());
    }

    #[test]
    fn rejects_empty_owner() {
        assert!(validate_owner_repo_format("/cli").is_err());
    }

    #[test]
    fn rejects_empty_repo() {
        assert!(validate_owner_repo_format("usedetail/").is_err());
    }

    #[test]
    fn rejects_multiple_slashes() {
        assert!(validate_owner_repo_format("a/b/c").is_err());
    }

    #[test]
    fn rejects_slash_only() {
        assert!(validate_owner_repo_format("/").is_err());
    }

    // ── match_repo_by_name ───────────────────────────────────────────

    fn sample_repos() -> Vec<Repo> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "repo_1", "name": "cli", "ownerName": "usedetail",
                "fullName": "usedetail/cli", "visibility": "public",
                "primaryBranch": "main", "orgId": "org_1", "orgName": "Detail"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "repo_2", "name": "cli", "ownerName": "acme",
                "fullName": "acme/cli", "visibility": "private",
                "primaryBranch": "main", "orgId": "org_2", "orgName": "Acme"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "repo_3", "name": "web", "ownerName": "usedetail",
                "fullName": "usedetail/web", "visibility": "public",
                "primaryBranch": "main", "orgId": "org_1", "orgName": "Detail"
            }))
            .unwrap(),
        ]
    }

    #[test]
    fn match_single_repo_by_name() {
        let repos = sample_repos();
        let id = match_repo_by_name("web", &repos).unwrap();
        assert_eq!(id.to_string(), "repo_3");
    }

    #[test]
    fn match_no_repo_returns_error() {
        let repos = sample_repos();
        let err = match_repo_by_name("nonexistent", &repos).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn match_multiple_repos_returns_error_with_names() {
        let repos = sample_repos();
        let err = match_repo_by_name("cli", &repos).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Multiple repositories"));
        assert!(msg.contains("usedetail/cli"));
        assert!(msg.contains("acme/cli"));
    }

    #[test]
    fn match_empty_repo_list() {
        let err = match_repo_by_name("cli", &[]).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

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
}
