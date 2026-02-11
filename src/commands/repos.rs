use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum RepoCommands {
    /// List all repositories you have access to
    List {
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
}

pub async fn handle(command: &RepoCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        RepoCommands::List { limit, page, format } => {
            let offset = crate::utils::page_to_offset(*page, *limit);

            let repos = client.list_repos(*limit, offset).await
                .context("Failed to fetch repositories")?;

            crate::output::output_list(&repos.repos, repos.total, format)
        }
    }
}
