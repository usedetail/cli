use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::*;

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Login with an API token
    Login {
        /// API token (dtl_live_...)
        #[arg(long)]
        token: Option<String>,
    },

    /// Logout and remove stored credentials
    Logout,

    /// Show current authentication status
    Status,
}

pub async fn handle(command: &AuthCommands, cli: &crate::Cli) -> Result<()> {
    match command {
        AuthCommands::Login { token } => {
            let token = match token {
                Some(t) => t.clone(),
                None => {
                    // Prompt for token
                    use console::Term;
                    let term = Term::stdout();
                    term.write_line("Please enter your API token:")?;
                    term.read_line()?
                }
            };

            // Validate token format
            if !token.starts_with("dtl_") {
                bail!("Invalid token format. Token should start with 'dtl_'");
            }

            // Test the token by making an API call
            let client = crate::api::client::ApiClient::new(
                cli.api_url.clone(),
                Some(token.clone()),
            )?;

            let user_info = client
                .get_current_user()
                .await
                .context("Failed to authenticate. Please check your token.")?;

            // Store token securely
            crate::config::storage::store_token(&token)?;

            println!("{}", "✓ Successfully authenticated!".green());
            println!("Logged in as: {}", user_info.email);
            println!("\nExample commands:");
            println!("  detail bugs list <repo_id>");
            println!("  detail bugs show <bug_id>");

            Ok(())
        }

        AuthCommands::Logout => {
            crate::config::storage::clear_credentials()?;
            println!("{}", "✓ Logged out successfully".green());
            Ok(())
        }

        AuthCommands::Status => {
            match cli.create_client() {
                Ok(client) => match client.get_current_user().await {
                    Ok(user) => {
                        println!("{}", "✓ Authenticated".green());
                        println!("Email: {}", user.email);
                        println!("API URL: {}", client.base_url());
                    }
                    Err(e) => {
                        println!("{}", "✗ Authentication invalid".red());
                        println!("Error: {}", e);
                        println!("\nRun `detail auth login` to re-authenticate");
                    }
                },
                Err(_) => {
                    println!("{}", "✗ Not authenticated".red());
                    println!("\nRun `detail auth login` to authenticate");
                }
            }
            Ok(())
        }
    }
}
