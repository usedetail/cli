#![deny(clippy::print_stdout)]

use anyhow::Result;
use clap::{Parser, Subcommand};

mod api;
mod commands;
mod config;
mod output;
mod upgrade;
mod utils;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "detail")]
#[command(version = VERSION)]
#[command(about = "Detail CLI - Manage bugs from your terminal", long_about = None)]
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
    /// Authentication commands
    Auth {
        #[command(subcommand)]
        command: commands::auth::AuthCommands,
    },

    /// Bug management commands
    Bugs {
        #[command(subcommand)]
        command: commands::bugs::BugCommands,
    },

    /// Repository management commands
    Repos {
        #[command(subcommand)]
        command: commands::repos::RepoCommands,
    },

    /// Show version information
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Auto-update in background (async, non-blocking)
    if let Err(e) = upgrade::auto_update().await {
        eprintln!("Warning: Failed to check for updates: {}", e);
    }

    match &cli.command {
        Commands::Auth { command } => commands::auth::handle(command, &cli).await,
        Commands::Bugs { command } => commands::bugs::handle(command, &cli).await,
        Commands::Repos { command } => commands::repos::handle(command, &cli).await,
        Commands::Version => {
            console::Term::stdout().write_line(&format!("detail-cli v{}", VERSION))?;
            Ok(())
        }
    }
}
