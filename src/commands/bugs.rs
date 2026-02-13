use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Term};

use crate::api::types::{BugDismissalReason, BugReviewState};

#[derive(Subcommand)]
pub enum BugCommands {
    /// List bugs
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

    /// Show bug details
    Show {
        /// Bug ID
        bug_id: String,
    },

    /// Mark bug as resolved or dismissed
    Review {
        /// Bug ID
        bug_id: String,

        /// Review state
        #[arg(long, value_enum)]
        state: BugReviewState,

        /// Dismissal reason (required if state is dismissed)
        #[arg(long, value_enum)]
        dismissal_reason: Option<BugDismissalReason>,

        /// Additional notes
        #[arg(long)]
        notes: Option<String>,
    },
}

/// Resolve owner/repo or repo name to repo ID
async fn resolve_repo_id(
    client: &crate::api::client::ApiClient,
    repo_identifier: &str,
) -> Result<crate::api::types::RepoId> {
    // If it contains a slash, validate as owner/repo format
    if repo_identifier.contains('/') {
        let parts: Vec<&str> = repo_identifier.split('/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            bail!(
                "Invalid repository format. Please use owner/repo (e.g., 'usedetail/cli') or just the repo name. Run 'detail repos list' to see your repositories."
            );
        }

        // Search for exact match on full_name
        let limit = 100;
        let mut offset = 0;

        loop {
            let repos = client
                .list_repos(limit, offset)
                .await
                .context("Failed to fetch repositories while resolving identifier")?;

            if let Some(repo) = repos.repos.iter().find(|r| r.full_name == repo_identifier) {
                return Ok(repo.id.clone());
            }

            if repos.repos.len() < limit as usize {
                break;
            }

            offset += limit;
        }

        bail!(
            "Repository '{}' not found. Make sure you have access to this repository.",
            repo_identifier
        )
    } else {
        // Repo without owner, search all repos and collect matches
        let limit = 100;
        let mut offset = 0;
        let mut matching_repos = Vec::new();

        loop {
            let repos = client
                .list_repos(limit, offset)
                .await
                .context("Failed to fetch repositories while resolving identifier")?;

            let page_size = repos.repos.len();

            // Collect all repos with matching name
            for repo in repos.repos {
                if repo.name == repo_identifier {
                    matching_repos.push(repo);
                }
            }

            if page_size < limit as usize {
                break;
            }

            offset += limit;
        }

        match matching_repos.len() {
            0 => bail!(
                "Repository '{}' not found. Run 'detail repos list' to see your repositories.",
                repo_identifier
            ),
            1 => Ok(matching_repos[0].id.clone()),
            _ => {
                let repo_list: Vec<String> = matching_repos
                    .iter()
                    .map(|r| format!("  - {}", r.full_name))
                    .collect();

                bail!(
                    "Multiple repositories with name '{}' found:\n{}\n\nPlease specify using owner/repo format (e.g., '{}').",
                    repo_identifier,
                    repo_list.join("\n"),
                    matching_repos[0].full_name
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

            let offset = crate::utils::page_to_offset(*page, *limit);

            let bugs = client
                .list_bugs(&resolved_repo_id, Some(status), *limit, offset)
                .await
                .context("Failed to fetch bugs from repository")?;

            crate::output::output_list(&bugs.bugs, bugs.total, *page, *limit, format)
        }

        BugCommands::Show { bug_id } => {
            use crate::api::types::BugId;

            let bug_id = BugId::new(bug_id).map_err(|e| anyhow::anyhow!(e))?;

            let bug = client
                .get_bug(&bug_id)
                .await
                .context("Failed to fetch bug details")?;

            let mut pairs: Vec<(&str, String)> = vec![
                ("ID", bug.id.to_string()),
                ("Title", bug.title.clone()),
                ("File", bug.file_path.as_deref().unwrap_or("-").to_string()),
                ("Created", crate::utils::format_datetime(bug.created_at)),
                (
                    "Security",
                    bug.is_security_vulnerability
                        .map(|v| if v { "Yes" } else { "No" })
                        .unwrap_or("-")
                        .to_string(),
                ),
            ];
            if let Some(review) = &bug.review {
                pairs.push(("Review", review.state.to_string()));
                pairs.push((
                    "Review Date",
                    crate::utils::format_datetime(review.created_at),
                ));
                if let Some(reason) = &review.dismissal_reason {
                    pairs.push(("Dismissal", reason.to_string()));
                }
                if let Some(notes) = &review.notes {
                    pairs.push(("Notes", notes.clone()));
                }
            }
            crate::output::SectionRenderer::new()
                .key_value("", &pairs)
                .markdown("", &bug.summary)
                .print()
        }

        BugCommands::Review {
            bug_id,
            state,
            dismissal_reason,
            notes,
        } => {
            use crate::api::types::BugId;

            // Validate that dismissal_reason is provided when state is dismissed
            if matches!(state, BugReviewState::Dismissed) && dismissal_reason.is_none() {
                bail!("--dismissal-reason is required when state is 'dismissed'");
            }

            let bug_id = BugId::new(bug_id).map_err(|e| anyhow::anyhow!(e))?;

            client
                .update_bug_review(
                    &bug_id,
                    state.clone(),
                    dismissal_reason.clone(),
                    notes.as_deref(),
                )
                .await
                .context("Failed to update bug review")?;

            Term::stdout()
                .write_line(&format!(
                    "{}",
                    style(format!("✓ Updated bug review to: {}", state)).green()
                ))
                .ok();
            Ok(())
        }
    }
}
