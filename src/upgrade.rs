use std::fs::File;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axoupdater::{AxoUpdater, UpdateResult};
use console::{style, Term};
use fs2::FileExt;

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

fn update_lock_path() -> Result<PathBuf> {
    storage::config_path().map(|p| p.with_file_name("update.lock"))
}

/// Try to acquire the update lock without blocking.
/// Returns `Some(file)` if the lock was acquired, `None` if another process holds it.
fn try_acquire_update_lock() -> Result<Option<File>> {
    let lock_path = update_lock_path()?;
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Acquire the update lock, blocking until it is available.
fn acquire_update_lock() -> Result<File> {
    let lock_path = update_lock_path()?;
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    file.lock_exclusive()?;
    Ok(file)
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
    let Some(_lock) = try_acquire_update_lock()? else {
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
    let _lock = acquire_update_lock()?;

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
    use std::sync::Mutex;

    use crate::config::storage::Config;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_config<F: FnOnce() -> R, R>(f: F) -> R {
        use std::{env, fs, process};

        let _guard = ENV_LOCK.lock().unwrap();
        let dir = env::temp_dir().join(format!("detail-cli-upgrade-test-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        let prev = env::var("XDG_CONFIG_HOME").ok();
        env::set_var("XDG_CONFIG_HOME", &dir);

        let result = f();

        match prev {
            Some(v) => env::set_var("XDG_CONFIG_HOME", v),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = fs::remove_dir_all(&dir);
        result
    }

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

    #[test]
    fn claim_returns_true_on_first_call() {
        with_temp_config(|| {
            assert!(claim_update_check(100_000).unwrap());
            let config = storage::load_config().unwrap();
            assert_eq!(config.last_update_check, Some(100_000));
        });
    }

    #[test]
    fn claim_returns_false_within_interval() {
        with_temp_config(|| {
            assert!(claim_update_check(100_000).unwrap());
            assert!(!claim_update_check(100_000 + UPDATE_CHECK_INTERVAL - 1).unwrap());
        });
    }

    #[test]
    fn claim_returns_true_after_interval() {
        with_temp_config(|| {
            assert!(claim_update_check(100_000).unwrap());
            assert!(claim_update_check(100_000 + UPDATE_CHECK_INTERVAL).unwrap());
        });
    }

    #[test]
    fn try_lock_returns_none_when_already_held() {
        with_temp_config(|| {
            let first = try_acquire_update_lock().unwrap();
            assert!(first.is_some());
            let second = try_acquire_update_lock().unwrap();
            assert!(second.is_none());
        });
    }

    #[test]
    fn lock_released_on_drop() {
        with_temp_config(|| {
            {
                let lock = try_acquire_update_lock().unwrap();
                assert!(lock.is_some());
            }
            // After drop, another acquire should succeed
            let lock = try_acquire_update_lock().unwrap();
            assert!(lock.is_some());
        });
    }
}
