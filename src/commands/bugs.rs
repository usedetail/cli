use anyhow::{bail, Context, Result};
use clap::{Subcommand, ValueEnum};
use colored::*;

#[derive(Clone, ValueEnum)]
pub enum ReviewState {
    /// Mark as pending (reopen)
    Pending,
    /// Mark as resolved
    Resolved,
    /// Mark as dismissed
    Dismissed,
}

impl ReviewState {
    fn as_str(&self) -> &str {
        match self {
            ReviewState::Pending => "pending",
            ReviewState::Resolved => "resolved",
            ReviewState::Dismissed => "dismissed",
        }
    }
}

#[derive(Clone, ValueEnum)]
pub enum DismissalReason {
    /// Not a bug
    NotABug,
    /// Won't fix
    WontFix,
    /// Duplicate issue
    Duplicate,
    /// Other reason
    Other,
}

impl DismissalReason {
    fn as_str(&self) -> &str {
        match self {
            DismissalReason::NotABug => "not_a_bug",
            DismissalReason::WontFix => "wont_fix",
            DismissalReason::Duplicate => "duplicate",
            DismissalReason::Other => "other",
        }
    }
}

#[derive(Clone, ValueEnum)]
pub enum BugStatus {
    Pending,
    Resolved,
    Dismissed,
}

impl BugStatus {
    fn as_str(&self) -> &str {
        match self {
            BugStatus::Pending => "pending",
            BugStatus::Resolved => "resolved",
            BugStatus::Dismissed => "dismissed",
        }
    }
}

#[derive(Subcommand)]
pub enum BugCommands {
    /// List bugs
    List {
        /// Repository ID or owner/repo
        repo: String,

        /// Status filter
        #[arg(long, value_enum, default_value = "pending")]
        status: BugStatus,

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
        state: ReviewState,

        /// Dismissal reason (required if state is dismissed)
        #[arg(long, value_enum)]
        dismissal_reason: Option<DismissalReason>,

        /// Additional notes
        #[arg(long)]
        notes: Option<String>,
    },
}

/// Resolve repo identifier to repo ID
/// Accepts either a repo ID (uuid) or owner/repo format
async fn resolve_repo_id(
    client: &crate::api::client::ApiClient,
    repo_identifier: &str,
) -> Result<crate::api::types::RepoId> {
    use crate::api::types::RepoId;

    // If it contains a slash, treat it as owner/repo format
    if repo_identifier.contains('/') {
        // Paginate through all repos to find the matching one
        let limit = 100;
        let mut offset = 0;

        loop {
            let repos = client.list_repos(limit, offset).await
                .context("Failed to fetch repositories while resolving identifier")?;

            // Check if we found the repo in this page
            if let Some(repo) = repos.repos.iter().find(|r| r.full_name == repo_identifier) {
                return Ok(repo.id.clone());
            }

            // If we got fewer results than the limit, we've reached the end
            if repos.repos.len() < limit as usize {
                break;
            }

            // Move to next page
            offset += limit;
        }

        // Repo not found after checking all pages
        bail!(
            "Repository '{}' not found. Make sure you have access to this repository.",
            repo_identifier
        )
    } else {
        // Assume it's already a repo ID and validate it
        RepoId::new(repo_identifier)
            .map_err(|e| anyhow::anyhow!(e))
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
            // Resolve repo identifier to ID (handles both UUID and owner/repo format)
            let resolved_repo_id = resolve_repo_id(&client, repo).await
                .context("Failed to resolve repository identifier")?;

            let offset = crate::utils::page_to_offset(*page, *limit);

            let bugs = client
                .list_bugs(&resolved_repo_id, Some(status.as_str()), *limit, offset)
                .await
                .context("Failed to fetch bugs from repository")?;

            crate::output::output_list(&bugs.bugs, bugs.total, format)
        }

        BugCommands::Show { bug_id } => {
            use crate::api::types::BugId;

            let bug_id = BugId::new(bug_id)
                .map_err(|e| anyhow::anyhow!(e))?;

            let bug = client.get_bug(&bug_id).await
                .context("Failed to fetch bug details")?;

            println!("{}", "Bug Details".bold());
            println!("ID:       {}", bug.id);
            println!("Title:    {}", bug.title);
            println!("Report:   {}", bug.summary);
            println!("File:     {}", bug.file_path.as_deref().unwrap_or("-"));
            println!("Created:  {}", crate::utils::format_datetime(bug.created_at));
            println!(
                "Security: {}",
                bug.is_security_vulnerability
                    .map(|is_vuln| if is_vuln { "Yes" } else { "No" })
                    .unwrap_or("-")
            );
            if let Some(review) = bug.review {
                println!("\nReview:");
                println!("  State:  {}", review.state);
                println!("  Date:   {}", crate::utils::format_datetime(review.created_at));
                if let Some(reason) = review.dismissal_reason {
                    println!("  Reason: {}", reason);
                }
                if let Some(notes) = review.notes {
                    println!("  Notes:  {}", notes);
                }
            }

            Ok(())
        }

        BugCommands::Review {
            bug_id,
            state,
            dismissal_reason,
            notes,
        } => {
            use crate::api::types::BugId;

            // Validate that dismissal_reason is provided when state is dismissed
            if matches!(state, ReviewState::Dismissed) && dismissal_reason.is_none() {
                bail!("--dismissal-reason is required when state is 'dismissed'");
            }

            let bug_id = BugId::new(bug_id)
                .map_err(|e| anyhow::anyhow!(e))?;

            let dismissal_reason_str = dismissal_reason.as_ref().map(|r| r.as_str());

            client
                .update_bug_review(&bug_id, state.as_str(), dismissal_reason_str, notes.as_deref())
                .await
                .context("Failed to update bug review")?;

            println!("{}", format!("✓ Updated bug review to: {}", state.as_str()).green());
            Ok(())
        }
    }
}
