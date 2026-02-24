use anyhow::Result;
use console::{style, Term};

use crate::upgrade::{self, ManualUpdateOutcome};

pub async fn handle() -> Result<()> {
    let term = Term::stdout();
    term.write_line("Checking for Detail CLI updates...")?;

    match upgrade::update_now().await? {
        ManualUpdateOutcome::Updated {
            old_version,
            new_version,
        } => {
            upgrade::print_update_success_message(&term, &old_version, &new_version);
        }
        ManualUpdateOutcome::AlreadyUpToDate => {
            term.write_line(&format!(
                "{}",
                style("✓ Detail CLI is already up to date.").green()
            ))?;
        }
        ManualUpdateOutcome::Unavailable => {
            term.write_line(&format!(
                "{}",
                style("Self-update is not available for this installation.").yellow()
            ))?;
            term.write_line("Reinstall using the official installer to enable `detail update`:")?;
            term.write_line("  curl --proto '=https' --tlsv1.2 -LsSf https://cli.detail.dev | sh")?;
        }
    }

    Ok(())
}
