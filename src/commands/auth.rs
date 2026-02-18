use anyhow::{bail, Context, Result};
use clap::Subcommand;
use console::{style, Key, Term};

use crate::api::client::ApiClient;
use crate::config::storage;

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
                    let term = Term::stdout();
                    term.write_str("Paste your API token: ")?;

                    // Read character-by-character (hidden) and auto-submit
                    // when a complete token is detected
                    let mut token = String::new();
                    loop {
                        let key = term.read_key()?;
                        match key {
                            Key::Char(c) if !c.is_whitespace() => {
                                token.push(c);
                                if is_complete_token(&token) {
                                    term.write_line("")?;
                                    break;
                                }
                            }
                            Key::Enter => {
                                term.write_line("")?;
                                break;
                            }
                            Key::Backspace => {
                                token.pop();
                            }
                            _ => {}
                        }
                    }
                    token
                }
            };

            // Validate token format
            if !token.starts_with("dtl_") {
                bail!("Invalid token format. Token should start with 'dtl_'");
            }

            // Test the token by making an API call
            let client = ApiClient::new(cli.api_url.clone(), Some(token.clone()))?;

            let user_info = client
                .get_current_user()
                .await
                .context("Failed to authenticate. Please check your token.")?;

            // Store token securely
            storage::store_token(&token)?;

            let term = Term::stdout();
            term.write_line(&format!(
                "{}",
                style("✓ Successfully authenticated!").green()
            ))?;
            term.write_line(&format!("Logged in as: {}", user_info.email))?;
            term.write_line("\nExample commands:")?;
            term.write_line("  detail bugs list <owner>/<repo>")?;
            term.write_line("  detail bugs show <bug_id>")?;

            Ok(())
        }

        AuthCommands::Logout => {
            storage::clear_credentials()?;
            Term::stdout()
                .write_line(&format!("{}", style("✓ Logged out successfully").green()))?;
            Ok(())
        }

        AuthCommands::Status => {
            match cli.create_client() {
                Ok(client) => {
                    let term = Term::stdout();
                    match client.get_current_user().await {
                        Ok(user) => {
                            term.write_line(&format!("{}", style("✓ Authenticated").green()))?;
                            term.write_line(&format!("Email: {}", user.email))?;
                        }
                        Err(e) => {
                            term.write_line(&format!(
                                "{}",
                                style("✗ Authentication invalid").red()
                            ))?;
                            term.write_line(&format!("Error: {}", e))?;
                            term.write_line("\nRun `detail auth login` to re-authenticate")?;
                        }
                    }
                }
                Err(_) => {
                    let term = Term::stdout();
                    term.write_line(&format!("{}", style("✗ Not authenticated").red()))?;
                    term.write_line("\nRun `detail auth login` to authenticate")?;
                }
            }
            Ok(())
        }
    }
}

/// Check if the string matches the expected API token format: dtl_{env}_{32hex}.{64hex}
fn is_complete_token(s: &str) -> bool {
    if !s.starts_with("dtl_") {
        return false;
    }
    let Some(dot_pos) = s.rfind('.') else {
        return false;
    };
    let after_dot = &s[dot_pos + 1..];
    after_dot.len() == 64 && after_dot.chars().all(|c| c.is_ascii_hexdigit())
}
