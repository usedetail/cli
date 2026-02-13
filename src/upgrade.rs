use anyhow::Result;
use axoupdater::AxoUpdater;
use console::{style, Term};

const UPDATE_CHECK_INTERVAL: u64 = 86400; // 24 hours in seconds

/// Automatically check for and install updates in the background
pub async fn auto_update() -> Result<()> {
    // Check if we should check for updates
    let mut config = crate::config::storage::load_config()?;

    if !config.check_for_updates {
        return Ok(());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    if let Some(last_check) = config.last_update_check {
        if now - last_check < UPDATE_CHECK_INTERVAL {
            return Ok(()); // Checked recently
        }
    }

    // Update last check time
    config.last_update_check = Some(now);
    crate::config::storage::save_config(&config)?;

    // Automatically update using axoupdater
    // This uses the install receipt created by cargo-dist
    match AxoUpdater::new_for("detail-cli").load_receipt() {
        Ok(updater) => {
            match updater.run().await {
                Ok(update_result) => {
                    if let Some(result) = update_result {
                        // Update was installed, binary on disk is now updated
                        print_update_success(&result);
                    }
                    // If None, already on latest version (silent)
                }
                Err(_) => {
                    // Silently ignore errors (update check is not critical)
                }
            }
        }
        Err(_) => {
            // No receipt found, probably not installed via cargo-dist installer
            // Skip the update check
        }
    }

    Ok(())
}

fn print_update_success(result: &axoupdater::UpdateResult) {
    let old_version = result
        .old_version
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let new_version = result.new_version.to_string();

    let term = Term::stderr();
    let _ = term.write_line("");
    let _ = term.write_line(&format!("{}", style("─".repeat(60)).dim()));
    let _ = term.write_line(&format!(
        "{}",
        style(format!(
            "✓ Updated Detail CLI from v{} to v{}",
            old_version, new_version
        ))
        .green()
    ));
    let _ = term.write_line(&format!(
        "{}",
        style("  Changes will apply on next run").dim()
    ));
    let _ = term.write_line(&format!("{}", style("─".repeat(60)).dim()));
    let _ = term.write_line("");
}
