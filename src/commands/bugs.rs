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

/// Resolve owner/repo or repo name to repo ID
async fn resolve_repo_id(client: &ApiClient, repo_identifier: &str) -> Result<RepoId> {
    if repo_identifier.contains('/') {
        let parts: Vec<&str> = repo_identifier.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            bail!(
                "Invalid repository format. Please use owner/repo (e.g., 'usedetail/cli') or just the repo name. Run 'detail repos list' to see your repositories."
            );
        }

        let repos = fetch_all_repos(client).await?;
        repos
            .iter()
            .find(|r| r.full_name == repo_identifier)
            .map(|r| r.id.clone())
            .context(format!(
                "Repository '{}' not found. Make sure you have access to this repository.",
                repo_identifier
            ))
    } else {
        let repos = fetch_all_repos(client).await?;
        let matching: Vec<_> = repos.iter().filter(|r| r.name == repo_identifier).collect();

        match matching.len() {
            0 => bail!(
                "Repository '{}' not found. Run 'detail repos list' to see your repositories.",
                repo_identifier
            ),
            1 => Ok(matching[0].id.clone()),
            _ => {
                let repo_list: Vec<String> = matching
                    .iter()
                    .map(|r| format!("  - {}", r.full_name))
                    .collect();
                bail!(
                    "Multiple repositories with name '{}' found:\n{}\n\nPlease specify using owner/repo format (e.g., '{}').",
                    repo_identifier,
                    repo_list.join("\n"),
                    matching[0].full_name
                )
            }
        }
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

            // Reject --state pending (only used as a list filter)
            if matches!(state, Some(BugReviewState::Pending)) {
                bail!("'pending' is not a valid close state. Use 'resolved' or 'dismissed'.");
            }

            // Resolve state: flag → prompt → error
            let state = match state {
                Some(s) => *s,
                None if is_interactive => prompt_close_state()?,
                None => bail!(
                    "--state is required in non-interactive mode. Use --state resolved or --state dismissed."
                ),
            };

            // Resolve dismissal_reason (only when dismissed)
            let dismissal_reason = if matches!(state, BugReviewState::Dismissed) {
                match dismissal_reason {
                    Some(r) => Some(*r),
                    None if is_interactive => Some(prompt_dismissal_reason()?),
                    None => bail!(
                        "--dismissal-reason is required when state is 'dismissed' in non-interactive mode."
                    ),
                }
            } else {
                *dismissal_reason
            };

            // Resolve notes: flag → prompt → None
            let notes = match notes {
                Some(n) => Some(n.clone()),
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
