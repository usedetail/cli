use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axoupdater::{AxoUpdater, UpdateResult};
use console::{style, Term};

use crate::config::storage;

const UPDATE_CHECK_INTERVAL: u64 = 3600; // 1 hour in seconds

#[derive(Debug, PartialEq, Eq)]
pub enum ManualUpdateOutcome {
    Updated {
        old_version: String,
        new_version: String,
    },
    AlreadyUpToDate,
    Unavailable,
}

fn version_strings(result: &UpdateResult) -> (String, String) {
    let old_version = result
        .old_version
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let new_version = result.new_version.to_string();
    (old_version, new_version)
}

pub(crate) fn print_update_success_message(term: &Term, old_version: &str, new_version: &str) {
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

fn should_check_for_updates(config: &storage::Config, now: u64) -> bool {
    if !config.check_for_updates {
        return false;
    }

    match config.last_update_check {
        Some(last_check) => now.saturating_sub(last_check) >= UPDATE_CHECK_INTERVAL,
        None => true,
    }
}

fn now_unix_seconds() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn load_configured_updater() -> Option<AxoUpdater> {
    let mut updater = AxoUpdater::new_for("detail-cli");
    updater.load_receipt().ok()?;
    Some(updater)
}

fn record_update_check_now() -> Result<()> {
    let mut config = storage::load_config()?;
    config.last_update_check = Some(now_unix_seconds()?);
    storage::save_config(&config)?;
    Ok(())
}

/// Automatically check for and install updates in the background
pub async fn auto_update() -> Result<()> {
    // Check if we should check for updates
    let mut config = storage::load_config()?;

    let now = now_unix_seconds()?;

    if !should_check_for_updates(&config, now) {
        return Ok(());
    }

    // Update last check time
    config.last_update_check = Some(now);
    storage::save_config(&config)?;

    // Automatically update using axoupdater
    // This uses the install receipt created by cargo-dist
    if let Some(mut updater) = load_configured_updater() {
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
    } else {
        // No receipt found, probably not installed via cargo-dist installer
        // Skip the update check
    }

    Ok(())
}

pub async fn update_now() -> Result<ManualUpdateOutcome> {
    record_update_check_now()?;

    let Some(mut updater) = load_configured_updater() else {
        return Ok(ManualUpdateOutcome::Unavailable);
    };

    match updater
        .run()
        .await
        .context("Failed to run updater for Detail CLI")?
    {
        Some(result) => {
            let (old_version, new_version) = version_strings(&result);

            Ok(ManualUpdateOutcome::Updated {
                old_version,
                new_version,
            })
        }
        None => Ok(ManualUpdateOutcome::AlreadyUpToDate),
    }
}

fn print_update_success(result: &UpdateResult) {
    let (old_version, new_version) = version_strings(result);
    print_update_success_message(&Term::stderr(), &old_version, &new_version);
}

#[cfg(test)]
mod tests {
    use crate::config::storage::Config;

    use super::*;

    fn base_config() -> Config {
        Config {
            api_url: None,
            check_for_updates: true,
            last_update_check: None,
            api_token: None,
        }
    }

    #[test]
    fn should_skip_when_updates_disabled() {
        let mut config = base_config();
        config.check_for_updates = false;
        assert!(!should_check_for_updates(&config, 10_000));
    }

    #[test]
    fn should_check_on_first_run_when_enabled() {
        let config = base_config();
        assert!(should_check_for_updates(&config, 10_000));
    }

    #[test]
    fn should_skip_when_checked_recently() {
        let mut config = base_config();
        config.last_update_check = Some(10_000);
        assert!(!should_check_for_updates(
            &config,
            10_000 + UPDATE_CHECK_INTERVAL - 1
        ));
    }

    #[test]
    fn should_check_when_interval_elapsed() {
        let mut config = base_config();
        config.last_update_check = Some(10_000);
        assert!(should_check_for_updates(
            &config,
            10_000 + UPDATE_CHECK_INTERVAL
        ));
    }

    #[test]
    fn should_skip_when_clock_moves_backwards() {
        let mut config = base_config();
        config.last_update_check = Some(10_000);
        assert!(!should_check_for_updates(&config, 9_000));
    }
}
