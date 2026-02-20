use std::collections::BTreeMap;

use anyhow::{Context, Result};
use clap::Subcommand;
use console::{style, Term};

use crate::output::output_list;
use crate::utils::page_to_offset;

#[derive(Subcommand)]
pub enum RepoCommands {
    /// List all repositories you have access to
    List {
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

pub async fn handle(command: &RepoCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        RepoCommands::List {
            limit,
            page,
            format,
        } => {
            let offset = page_to_offset(*page, *limit);

            let repos = client
                .list_repos(*limit, offset)
                .await
                .context("Failed to fetch repositories")?;

            match format {
                crate::OutputFormat::Table => {
                    let term = Term::stdout();
                    let width = term.size().1 as usize;
                    let separator = "─".repeat(width);

                    // Group repos by organization, sorted alphabetically
                    let mut by_org: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                    for repo in &repos.repos {
                        by_org.entry(&repo.org_name).or_default().push(&repo.name);
                    }

                    for (org_name, repo_names) in &by_org {
                        term.write_line(&format!("{} {}", style("Organization").bold(), org_name))?;
                        term.write_line(&format!("{}", style(&separator).dim()))?;
                        for name in repo_names {
                            term.write_line(&format!("- {}", name))?;
                        }
                        term.write_line("")?;
                    }

                    let total_pages = (repos.total.max(0) as u32).div_ceil(*limit).max(1);
                    term.write_line(&format!("Page: {} of {}", page, total_pages))?;
                    Ok(())
                }
                _ => output_list(&repos.repos, repos.total.max(0) as usize, *page, *limit, format),
            }
        }
    }
}
