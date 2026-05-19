#![deny(clippy::print_stdout, clippy::print_stderr, clippy::absolute_paths)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::as_conversions,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::needless_collect,
        clippy::absolute_paths,
        clippy::if_then_some_else_none,
        clippy::doc_markdown,
        clippy::semicolon_outside_block,
        reason = "restriction/pedantic lints relaxed in test cfg — unwrap/expect/panic/casts and minor stylistic lints are idiomatic in tests"
    )
)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

pub mod api;
pub mod commands;
pub mod config;
pub mod output;
pub mod upgrade;
pub mod utils;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const LONG_ABOUT: &str = "\
Detail CLI - Manage bugs from your terminal

Common workflow:
  1. List pending bugs:   detail bugs list <owner/repo>
  2. View a bug report:   detail bugs show <bug_id>
  3. Fix the bug
  4. Close the bug:       detail bugs close <bug_id>";

const COMPLETIONS_LONG_ABOUT: &str = "\
Print shell completion script to stdout.

Add the appropriate line to your shell's startup file:

  bash (~/.bashrc):
    source <(detail completions bash)

  zsh (~/.zshrc):
    source <(detail completions zsh)

  fish (~/.config/fish/config.fish):
    detail completions fish | source

  powershell ($PROFILE):
    detail completions powershell | Out-String | Invoke-Expression

SHELL defaults to whatever is detected from $SHELL. Supported shells:
bash, zsh, fish, elvish, powershell.";

#[derive(Parser)]
#[command(name = "detail")]
#[command(version = VERSION)]
#[command(about = "Detail CLI - Manage bugs from your terminal")]
#[command(long_about = LONG_ABOUT)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Create an authenticated API client
    pub fn create_client(&self) -> Result<api::client::ApiClient> {
        let config = config::storage::load_config()?;
        let token = config
            .api_token
            .context("No token found. Run `detail auth login`")?;
        api::client::ApiClient::new(config.api_url, Some(token))
    }

    const fn is_json(format: &OutputFormat) -> bool {
        matches!(format, OutputFormat::Json)
    }

    /// Returns true when machine-readable output is requested (e.g. `--format json`),
    /// meaning non-essential messages (update notices, progress) should be suppressed
    /// to avoid corrupting structured output.
    const fn is_silent(&self) -> bool {
        match &self.command {
            Commands::Bugs { command } => match command {
                commands::bugs::BugCommands::List { format, .. }
                | commands::bugs::BugCommands::Show { format, .. }
                | commands::bugs::BugCommands::Close { format, .. } => Self::is_json(format),
                commands::bugs::BugCommands::Reopen { .. } => false,
            },
            Commands::Repos { command } => match command {
                commands::repos::RepoCommands::List { format, .. } => Self::is_json(format),
            },
            Commands::Scans { command } => match command {
                commands::scans::ScanCommands::List { format, .. } => Self::is_json(format),
            },
            Commands::Rules { command } => match command {
                commands::rules::RuleCommands::List { format, .. }
                | commands::rules::RuleCommands::Requests(
                    commands::rules::RuleRequestCommands::List { format, .. },
                ) => Self::is_json(format),
                commands::rules::RuleCommands::Create { .. }
                | commands::rules::RuleCommands::Propose { .. }
                | commands::rules::RuleCommands::Requests(_)
                | commands::rules::RuleCommands::Show { .. }
                | commands::rules::RuleCommands::Pull { .. } => false,
            },
            // Completions prints a shell snippet that may be sourced via
            // `source <(detail completions bash)` from the user's rc file, so
            // any auto-update notice on stderr would surface on every shell
            // startup — keep this silent.
            Commands::Completions { .. } => true,
            Commands::Auth { .. }
            | Commands::SatisfyingSort
            | Commands::Skill { .. }
            | Commands::Update
            | Commands::Version => false,
        }
    }

    const fn should_run_auto_update(&self) -> bool {
        if self.is_silent() {
            return false;
        }
        !matches!(&self.command, Commands::Update)
    }

    /// Run the CLI command
    pub async fn run(self) -> Result<()> {
        // Skip auto-update when outputting JSON to avoid corrupting structured output
        if self.should_run_auto_update() {
            if let Err(e) = upgrade::auto_update().await {
                let _ = console::Term::stderr()
                    .write_line(&format!("Warning: Failed to check for updates: {e}"));
            }
        }

        match &self.command {
            Commands::Auth { command } => commands::auth::handle(command, &self).await,
            Commands::Bugs { command } => commands::bugs::handle(command, &self).await,
            Commands::Completions { shell } => commands::completions::handle(shell.as_deref()),
            Commands::Rules { command } => commands::rules::handle(command, &self).await,
            Commands::SatisfyingSort => commands::satisfying_sort::handle().await,
            Commands::Repos { command } => commands::repos::handle(command, &self).await,
            Commands::Scans { command } => commands::scans::handle(command, &self).await,
            Commands::Skill { command } => commands::skill::handle(command.as_ref()),
            Commands::Update => commands::update::handle().await,
            Commands::Version => {
                console::Term::stdout().write_line(&format!("detail-cli v{VERSION}"))?;
                Ok(())
            }
        }
    }
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
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

    /// Print shell completion script to stdout
    #[command(long_about = COMPLETIONS_LONG_ABOUT)]
    Completions {
        /// Shell to print completions for (defaults to $SHELL)
        shell: Option<String>,
    },

    /// Create and inspect rules
    Rules {
        #[command(subcommand)]
        command: commands::rules::RuleCommands,
    },

    /// Run a fun animation. Humans only.
    #[command(name = "satisfying-sort")]
    SatisfyingSort,

    /// Manage repos tracked with Detail
    Repos {
        #[command(subcommand)]
        command: commands::repos::RepoCommands,
    },

    /// List and inspect scans
    Scans {
        #[command(subcommand)]
        command: commands::scans::ScanCommands,
    },

    /// Install Detail skills (default: detail-bugs)
    Skill {
        #[command(subcommand)]
        command: Option<commands::skill::SkillCommands>,
    },

    /// Update immediately (auto-update also runs in the background)
    Update,

    /// Show version information
    Version,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::BugReviewState;

    #[test]
    fn silent_when_bugs_list_json() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--format", "json"])
            .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_when_bugs_list_table() {
        let cli =
            Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--format", "table"])
                .unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_when_bugs_list_default_format() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_rules_list_json() {
        let cli =
            Cli::try_parse_from(["detail", "rules", "list", "owner/repo", "--format", "json"])
                .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_when_rules_list_table() {
        let cli =
            Cli::try_parse_from(["detail", "rules", "list", "owner/repo", "--format", "table"])
                .unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_rules_requests_list_json() {
        let cli = Cli::try_parse_from([
            "detail",
            "rules",
            "requests",
            "list",
            "owner/repo",
            "--format",
            "json",
        ])
        .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_when_rules_requests_list_table() {
        let cli = Cli::try_parse_from([
            "detail",
            "rules",
            "requests",
            "list",
            "owner/repo",
            "--format",
            "table",
        ])
        .unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_repos_list_json() {
        let cli = Cli::try_parse_from(["detail", "repos", "list", "--format", "json"]).unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_when_repos_list_table() {
        let cli = Cli::try_parse_from(["detail", "repos", "list", "--format", "table"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_for_bugs_show() {
        let cli = Cli::try_parse_from(["detail", "bugs", "show", "bug_123"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_bugs_show_json() {
        let cli =
            Cli::try_parse_from(["detail", "bugs", "show", "bug_123", "--format", "json"]).unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_for_bugs_close() {
        let cli =
            Cli::try_parse_from(["detail", "bugs", "close", "bug_123", "--state", "resolved"])
                .unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn bugs_reopen_parses() {
        let cli = Cli::try_parse_from(["detail", "bugs", "reopen", "bug_abc"]).unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::Reopen { bug_id },
        } = &cli.command
        {
            assert_eq!(bug_id, "bug_abc");
        } else {
            panic!("expected bugs reopen command");
        }
    }

    #[test]
    fn bugs_reopen_rejects_notes_flag() {
        // --notes was deliberately removed because the API replaces the
        // whole review row; passing notes here would just clobber the
        // existing review's notes. Keep this test until either the API
        // gains PATCH semantics or the CLI fetches-then-merges.
        let cli =
            Cli::try_parse_from(["detail", "bugs", "reopen", "bug_abc", "--notes", "anything"]);
        assert!(cli.is_err());
    }

    #[test]
    fn not_silent_for_bugs_reopen() {
        // Reopen has no JSON output to corrupt, so update notices stay on.
        let cli = Cli::try_parse_from(["detail", "bugs", "reopen", "bug_123"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_bugs_close_json() {
        let cli = Cli::try_parse_from([
            "detail", "bugs", "close", "bug_123", "--state", "resolved", "--format", "json",
        ])
        .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_for_auth_status() {
        let cli = Cli::try_parse_from(["detail", "auth", "status"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_for_version() {
        let cli = Cli::try_parse_from(["detail", "version"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_for_skill() {
        let cli = Cli::try_parse_from(["detail", "skill"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_for_skill_rules() {
        let cli = Cli::try_parse_from(["detail", "skill", "rules"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn completions_accepts_optional_shell_arg() {
        let cli = Cli::try_parse_from(["detail", "completions", "bash"]).unwrap();
        if let Commands::Completions { shell } = &cli.command {
            assert_eq!(shell.as_deref(), Some("bash"));
        } else {
            panic!("expected completions command");
        }
    }

    #[test]
    fn completions_shell_arg_optional() {
        let cli = Cli::try_parse_from(["detail", "completions"]).unwrap();
        if let Commands::Completions { shell } = &cli.command {
            assert!(shell.is_none());
        } else {
            panic!("expected completions command");
        }
    }

    #[test]
    fn silent_for_completions() {
        // Output is sourced by shell rc files via `source <(detail completions bash)`,
        // so auto-update notices must stay off.
        let cli = Cli::try_parse_from(["detail", "completions"]).unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_for_satisfying_sort() {
        let cli = Cli::try_parse_from(["detail", "satisfying-sort"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn not_silent_for_update() {
        let cli = Cli::try_parse_from(["detail", "update"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn auto_update_disabled_for_update_command() {
        let cli = Cli::try_parse_from(["detail", "update"]).unwrap();
        assert!(!cli.should_run_auto_update());
    }

    #[test]
    fn rejects_bugs_list_limit_above_api_max() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--limit", "101"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_repos_list_limit_above_api_max() {
        let cli = Cli::try_parse_from(["detail", "repos", "list", "--limit", "101"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_bugs_list_limit_zero() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--limit", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_repos_list_limit_zero() {
        let cli = Cli::try_parse_from(["detail", "repos", "list", "--limit", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn bugs_list_status_default_is_pending() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo"]).unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { status, .. },
        } = &cli.command
        {
            assert_eq!(status.len(), 1);
            assert!(matches!(status[0], BugReviewState::Pending));
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn bugs_list_status_comma_separated_parses() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--status",
            "pending,resolved",
        ])
        .unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { status, .. },
        } = &cli.command
        {
            assert_eq!(status.len(), 2);
            assert!(matches!(status[0], BugReviewState::Pending));
            assert!(matches!(status[1], BugReviewState::Resolved));
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn bugs_list_status_repeated_flag_parses() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--status",
            "resolved",
            "--status",
            "dismissed",
        ])
        .unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { status, .. },
        } = &cli.command
        {
            assert_eq!(status.len(), 2);
            assert!(matches!(status[0], BugReviewState::Resolved));
            assert!(matches!(status[1], BugReviewState::Dismissed));
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn rejects_bugs_list_page_zero() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--page", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn bugs_list_all_flag_parses() {
        let cli = Cli::try_parse_from(["detail", "bugs", "list", "owner/repo", "--all"]).unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { all, .. },
        } = &cli.command
        {
            assert!(*all);
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn bugs_list_all_silences_when_json() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--all",
            "--format",
            "json",
        ])
        .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn bugs_list_all_conflicts_with_page() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--all",
            "--page",
            "2",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn bugs_list_all_conflicts_with_limit() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--all",
            "--limit",
            "10",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn bugs_list_scan_id_parses() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--scan-id",
            "wr_abc123",
        ])
        .unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { scan_id, .. },
        } = &cli.command
        {
            assert_eq!(scan_id.as_deref(), Some("wr_abc123"));
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn bugs_list_since_until_parses() {
        let cli = Cli::try_parse_from([
            "detail",
            "bugs",
            "list",
            "owner/repo",
            "--since",
            "1d",
            "--until",
            "2024-01-15",
        ])
        .unwrap();
        if let Commands::Bugs {
            command: commands::bugs::BugCommands::List { since, until, .. },
        } = &cli.command
        {
            assert_eq!(since.as_deref(), Some("1d"));
            assert_eq!(until.as_deref(), Some("2024-01-15"));
        } else {
            panic!("expected bugs list command");
        }
    }

    #[test]
    fn rejects_repos_list_page_zero() {
        let cli = Cli::try_parse_from(["detail", "repos", "list", "--page", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn scans_list_parses() {
        let cli = Cli::try_parse_from(["detail", "scans", "list", "owner/repo"]).unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn silent_when_scans_list_json() {
        let cli =
            Cli::try_parse_from(["detail", "scans", "list", "owner/repo", "--format", "json"])
                .unwrap();
        assert!(cli.is_silent());
    }

    #[test]
    fn not_silent_when_scans_list_table() {
        let cli =
            Cli::try_parse_from(["detail", "scans", "list", "owner/repo", "--format", "table"])
                .unwrap();
        assert!(!cli.is_silent());
    }

    #[test]
    fn rejects_scans_list_limit_zero() {
        let cli = Cli::try_parse_from(["detail", "scans", "list", "owner/repo", "--limit", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_scans_list_limit_above_max() {
        let cli = Cli::try_parse_from(["detail", "scans", "list", "owner/repo", "--limit", "101"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_scans_list_page_zero() {
        let cli = Cli::try_parse_from(["detail", "scans", "list", "owner/repo", "--page", "0"]);
        assert!(cli.is_err());
    }

    #[test]
    fn rejects_unknown_api_url_flag() {
        // --api-url was removed; ensure it's no longer accepted.
        let cli = Cli::try_parse_from(["detail", "--api-url", "https://x.dev", "version"]);
        assert!(cli.is_err());
    }
}
