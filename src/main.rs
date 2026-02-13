#![deny(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Result;
use clap::{Parser, Subcommand};

mod api;
mod commands;
mod config;
mod output;
mod upgrade;
mod utils;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const LONG_ABOUT: &str = "\
Detail CLI - Manage bugs from your terminal

Common workflow:
  1. List pending bugs:   detail bugs list <owner/repo>
  2. View a bug report:   detail bugs show <bug_id>
  3. Fix the bug
  4. Close the bug:       detail bugs close <bug_id>";

#[derive(Parser)]
#[command(name = "detail")]
#[command(version = VERSION)]
#[command(about = "Detail CLI - Manage bugs from your terminal")]
#[command(long_about = LONG_ABOUT)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// API endpoint override (for testing)
    #[arg(long, env = "DETAIL_API_URL", global = true, hide = true)]
    api_url: Option<String>,
}

impl Cli {
    /// Create an authenticated API client
    pub fn create_client(&self) -> Result<api::client::ApiClient> {
        let token = config::storage::load_token()?;
        api::client::ApiClient::new(self.api_url.clone(), Some(token))
    }
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage login credentials
    Auth {
        #[command(subcommand)]
        command: commands::auth::AuthCommands,
    },

    /// List, show, and close bugs
    Bugs {
        #[command(subcommand)]
        command: commands::bugs::BugCommands,
    },

    /// Manage repos tracked with Detail
    Repos {
        #[command(subcommand)]
        command: commands::repos::RepoCommands,
    },

    /// Install the detail-bugs skill
    Skill,

    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Auto-update in background (async, non-blocking)
    if let Err(e) = upgrade::auto_update().await {
        let _ = console::Term::stderr()
            .write_line(&format!("Warning: Failed to check for updates: {}", e));
    }

    match &cli.command {
        Commands::Auth { command } => commands::auth::handle(command, &cli).await,
        Commands::Bugs { command } => commands::bugs::handle(command, &cli).await,
        Commands::Repos { command } => commands::repos::handle(command, &cli).await,
        Commands::Skill => commands::skill::handle(),
        Commands::Version => {
            console::Term::stdout().write_line(&format!("detail-cli v{}", VERSION))?;
            Ok(())
        }
    }
}
