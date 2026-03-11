use anyhow::{Context, Result};
use clap::Subcommand;

use crate::output::output_list;
use crate::utils::pagination::page_to_offset;
use crate::utils::repos::resolve_repo_id;

#[derive(Subcommand)]
pub enum ScanCommands {
    /// List recent scans for a repository
    List {
        /// Repository in owner/repo format or just repo name
        repo: String,

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

    /// Show bugs found in a specific scan
    Show {
        /// Workflow request ID of the scan (from `scans list` output)
        workflow_request_id: String,

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
}

pub async fn handle(command: &ScanCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        ScanCommands::List {
            repo,
            limit,
            page,
            format,
        } => {
            let repo_id = resolve_repo_id(&client, repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let offset = page_to_offset(*page, *limit);
            let scans = client
                .list_scans(&repo_id, *limit, offset)
                .await
                .context("Failed to fetch scans")?;

            output_list(
                &scans.scans,
                usize::try_from(scans.total.max(0)).unwrap_or(0),
                *page,
                *limit,
                format,
            )
        }
        ScanCommands::Show {
            workflow_request_id,
            limit,
            page,
            format,
        } => {
            let offset = page_to_offset(*page, *limit);
            let response = client
                .list_scan_bugs(workflow_request_id, *limit, offset)
                .await
                .context("Failed to fetch scan bugs")?;

            output_list(
                &response.bugs,
                usize::try_from(response.total.max(0)).unwrap_or(0),
                *page,
                *limit,
                format,
            )
        }
    }
}
