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
        .map_or_else(|| "unknown".to_string(), ToString::to_string);
    let new_version = result.new_version.to_string();
    (old_version, new_version)
}

pub(crate) fn print_update_success_message(term: &Term, old_version: &str, new_version: &str) {
    let _ = term.write_line("");
    let _ = term.write_line(&format!("{}", style("─".repeat(60)).dim()));
    let _ = term.write_line(&format!(
        "{}",
        style(format!(
            "✓ Updated Detail CLI from v{old_version} to v{new_version}"
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

const fn should_check_for_updates(config: &storage::Config, now: u64) -> bool {
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
    let now = now_unix_seconds()?;
    storage::update_config(|config| {
        config.last_update_check = Some(now);
    })
}

/// Atomically check whether an update is due and, if so, stamp the config
/// so concurrent processes will see a fresh timestamp and skip.
fn claim_update_check(now: u64) -> Result<bool> {
    let mut claimed = false;
    storage::update_config(|config| {
        if should_check_for_updates(config, now) {
            config.last_update_check = Some(now);
            claimed = true;
        }
    })?;
    Ok(claimed)
}

/// Automatically check for and install updates in the background
pub async fn auto_update() -> Result<()> {
    let now = now_unix_seconds()?;

    // Try to acquire a process-level lock. If another CLI instance is
    // already checking for / installing an update, skip silently.
    let Some(_lock) = storage::try_acquire_update_lock()? else {
        return Ok(());
    };

    // Atomically check the interval and stamp the config so that
    // concurrent processes that acquire the lock after us will see
    // a fresh timestamp and skip.
    if !claim_update_check(now)? {
        return Ok(());
    }

    // Perform the update with the lock held to prevent concurrent
    // binary replacement.
    if let Some(mut updater) = load_configured_updater() {
        if let Ok(Some(result)) = updater.run().await {
            print_update_success(&result);
        }
    }

    Ok(())
}

pub async fn update_now() -> Result<ManualUpdateOutcome> {
    // Acquire the update lock to prevent concurrent binary writes.
    // Block (rather than skip) since this is an explicit user request.
    let _lock = storage::acquire_update_lock()?;

    record_update_check_now()?;

    let Some(mut updater) = load_configured_updater() else {
        return Ok(ManualUpdateOutcome::Unavailable);
    };

    Ok(updater
        .run()
        .await
        .context("Failed to run updater for Detail CLI")?
        .map_or(ManualUpdateOutcome::AlreadyUpToDate, |result| {
            let (old_version, new_version) = version_strings(&result);
            ManualUpdateOutcome::Updated {
                old_version,
                new_version,
            }
        }))
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
            app_url: None,
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
