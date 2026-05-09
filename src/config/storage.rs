use std::fs::File;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, fs, process};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use toml_edit::ser::to_document;
use toml_edit::DocumentMut;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_url: Option<String>,
    pub app_url: Option<String>,
    pub check_for_updates: bool,
    pub last_update_check: Option<u64>,
    pub api_token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: None,
            app_url: None,
            check_for_updates: true,
            last_update_check: None,
            api_token: None,
        }
    }
}

/// Get the config file path, mirroring axoupdater's directory logic
/// to ensure config.toml is stored alongside the install receipt.
pub fn config_path() -> Result<PathBuf> {
    // Check XDG_CONFIG_HOME first (works on all platforms)
    let config_dir = if let Ok(xdg_config) = env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg_config).join("detail-cli")
    } else if cfg!(windows) {
        // Windows: use LOCALAPPDATA
        let local_app_data =
            env::var("LOCALAPPDATA").context("LOCALAPPDATA environment variable not set")?;
        PathBuf::from(local_app_data).join("detail-cli")
    } else {
        // Others: use ~/.config
        let home = homedir::my_home()
            .context("Failed to determine home directory")?
            .context("Home directory not found")?;
        home.join(".config").join("detail-cli")
    };

    fs::create_dir_all(&config_dir)?;
    Ok(config_dir.join("config.toml"))
}

pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).context("Failed to parse config")
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    let _lock = acquire_write_lock()?;
    let contents = toml::to_string_pretty(config)?;
    atomic_write(&path, contents.as_bytes())
}

/// Read-modify-write the config file, serialized against other writers and
/// published atomically so concurrent readers never observe a partial file.
///
/// Preserves comments and formatting the user may have added by hand. The
/// file is parsed as a `toml_edit::DocumentMut`; we snapshot the typed
/// `Config` before and after the closure and only touch keys whose value
/// actually changed. That keeps line comments above each key, trailing
/// inline comments, and the user's chosen quote style intact for fields
/// the update didn't care about — and leaves unknown user-added keys alone.
pub fn update_config(f: impl FnOnce(&mut Config)) -> Result<()> {
    let path = config_path()?;
    let _lock = acquire_write_lock()?;

    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };

    let mut doc: DocumentMut = contents.parse().context("Failed to parse config")?;
    let mut config: Config = toml::from_str(&contents).context("Failed to parse config")?;

    let before = toml::Table::try_from(&config).context("Failed to serialize config")?;
    f(&mut config);
    let after = toml::Table::try_from(&config).context("Failed to serialize config")?;
    let fresh = to_document(&config).context("Failed to serialize config")?;

    for (key, new_value) in &after {
        if before.get(key) != Some(new_value) {
            doc[key] = fresh[key].clone();
        }
    }
    for key in before.keys() {
        if !after.contains_key(key) {
            doc.remove(key);
        }
    }

    atomic_write(&path, doc.to_string().as_bytes())
}

/// Path of the lockfile used to serialize concurrent `save_config` /
/// `update_config` calls. We can't lock the config file itself: writers
/// publish via `rename(2)`, which swaps in a new inode — any lock on the
/// previous inode would no longer cover the visible path, so two concurrent
/// writers would each lock a different (already-unlinked) inode and race.
fn config_write_lock_path() -> Result<PathBuf> {
    config_path().map(|p| p.with_file_name("config.lock"))
}

fn acquire_write_lock() -> Result<File> {
    let lock_path = config_write_lock_path()?;
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    file.lock_exclusive()?;
    Ok(file)
}

/// Per-process counter for unique temp file names; combined with the PID
/// it lets concurrent writers (within or across processes) stage to
/// non-colliding paths.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Removes a staged temp file on the error path. Disarmed once `rename` has
/// successfully published the file.
struct TempCleanup<'a> {
    path: &'a Path,
    armed: bool,
}

impl Drop for TempCleanup<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.path);
        }
    }
}

/// Write `contents` to `path` atomically: stage to a temp file in the same
/// directory, then `rename` onto the target. Readers see either the complete
/// old file or the complete new file — never a partial write.
///
/// Mirrors the target's existing Unix mode (or falls back to 0600, since the
/// config can hold an API token). Cross-filesystem renames aren't atomic, so
/// the temp file must live in the same directory as the target.
///
/// We don't `fsync` the temp file: the bug we're fixing is reader/writer
/// visibility within a single uptime, not crash durability, and the original
/// in-place rewrite didn't fsync either. A power loss mid-update can still
/// leave the file empty, but `load_config` already treats that as defaults.
fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("config path has no parent directory")?;
    let mut tmp_name = path
        .file_name()
        .context("config path has no file name")?
        .to_os_string();
    tmp_name.push(format!(
        ".tmp.{}.{}",
        process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    let tmp_path = parent.join(tmp_name);

    let mut cleanup = TempCleanup {
        path: &tmp_path,
        armed: true,
    };

    let mut opts = File::options();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _ = opts.mode(0o600);
    }
    let mut file = opts.open(&tmp_path)?;
    file.write_all(contents)?;
    // Close before rename: on Windows `MoveFileExW` with REPLACE_EXISTING
    // tolerates open handles inconsistently across versions, and there's no
    // benefit to keeping the temp file open past this point.
    drop(file);

    // If there's an existing config, mirror its Unix mode so we don't silently
    // tighten or loosen what the user set.
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(path) {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(
            &tmp_path,
            fs::Permissions::from_mode(meta.permissions().mode()),
        )?;
    }

    fs::rename(&tmp_path, path)?;
    cleanup.armed = false;
    Ok(())
}

// Token storage in config file
pub fn store_token(token: &str) -> Result<()> {
    update_config(|config| {
        config.api_token = Some(token.to_string());
    })
}

pub fn load_token() -> Result<String> {
    let config = load_config()?;
    config
        .api_token
        .context("No token found. Run `detail auth login`")
}

pub fn clear_credentials() -> Result<()> {
    update_config(|config| {
        config.api_token = None;
    })
}

fn update_lock_path() -> Result<PathBuf> {
    config_path().map(|p| p.with_file_name("update.lock"))
}

/// Try to acquire the update lock without blocking.
/// Returns `Some(file)` if the lock was acquired, `None` if another process holds it.
pub fn try_acquire_update_lock() -> Result<Option<File>> {
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
pub fn acquire_update_lock() -> Result<File> {
    let lock_path = update_lock_path()?;
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    file.lock_exclusive()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use std::process;
    use std::sync::Mutex;

    use super::*;

    /// Mutex to serialize tests that modify the XDG_CONFIG_HOME env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Run a closure with XDG_CONFIG_HOME pointing to a fresh temp directory,
    /// restoring the original value afterwards.
    fn with_temp_config<F: FnOnce() -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = env::temp_dir().join(format!("detail-cli-test-{}", process::id()));
        let _ = fs::remove_dir_all(&dir); // clean slate
        let prev = env::var("XDG_CONFIG_HOME").ok();
        env::set_var("XDG_CONFIG_HOME", &dir);

        let result = f();

        // Restore
        match prev {
            Some(v) => env::set_var("XDG_CONFIG_HOME", v),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }
        let _ = fs::remove_dir_all(&dir);
        result
    }

    // ── Config TOML round-trip ───────────────────────────────────────

    #[test]
    fn config_default_serializes_to_toml() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("check_for_updates = true"));
    }

    #[test]
    fn config_round_trip_via_toml() {
        let config = Config {
            api_url: Some("https://api.example.com".into()),
            app_url: None,
            check_for_updates: true,
            last_update_check: Some(12345),
            api_token: Some("dtl_test_token".into()),
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let restored: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(restored.api_url.as_deref(), Some("https://api.example.com"));
        assert!(restored.check_for_updates);
        assert_eq!(restored.last_update_check, Some(12345));
        assert_eq!(restored.api_token.as_deref(), Some("dtl_test_token"));
    }

    #[test]
    fn config_missing_optional_fields_parse_as_none() {
        let toml_str = "check_for_updates = true\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.api_url.is_none());
        assert!(config.api_token.is_none());
        assert!(config.last_update_check.is_none());
    }

    #[test]
    fn config_missing_check_for_updates_defaults_to_true() {
        let toml_str = r#"api_url = "https://api.example.com""#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.check_for_updates);
        assert_eq!(config.api_url.as_deref(), Some("https://api.example.com"));
    }

    // ── config_path ──────────────────────────────────────────────────

    #[test]
    fn config_path_uses_xdg_config_home() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            assert!(path.ends_with("detail-cli/config.toml"));
            assert!(path.parent().unwrap().exists());
        });
    }

    // ── save / load round-trip ───────────────────────────────────────

    #[test]
    fn save_then_load_config() {
        with_temp_config(|| {
            let config = Config {
                api_url: Some("https://test.dev".into()),
                app_url: None,
                check_for_updates: true,
                last_update_check: None,
                api_token: Some("tok".into()),
            };
            save_config(&config).unwrap();
            let loaded = load_config().unwrap();
            assert_eq!(loaded.api_url.as_deref(), Some("https://test.dev"));
            assert_eq!(loaded.api_token.as_deref(), Some("tok"));
        });
    }

    #[test]
    fn load_config_partial_file_defaults_check_for_updates() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            fs::write(&path, r#"api_token = "tok""#).unwrap();

            let loaded = load_config().unwrap();
            assert!(loaded.check_for_updates);
            assert_eq!(loaded.api_token.as_deref(), Some("tok"));
            assert!(loaded.api_url.is_none());
            assert!(loaded.last_update_check.is_none());
        });
    }

    #[test]
    fn load_config_returns_defaults_when_no_file() {
        with_temp_config(|| {
            let config = load_config().unwrap();
            assert!(config.check_for_updates);
            assert!(config.api_token.is_none());
        });
    }

    #[test]
    fn load_config_invalid_toml_has_parse_context() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            fs::write(&path, "check_for_updates = maybe").unwrap();

            let err = load_config().unwrap_err();
            assert!(err.to_string().contains("Failed to parse config"));
        });
    }

    // ── token helpers ────────────────────────────────────────────────

    #[test]
    fn store_and_load_token() {
        with_temp_config(|| {
            store_token("dtl_live_secret").unwrap();
            assert_eq!(load_token().unwrap(), "dtl_live_secret");
        });
    }

    #[test]
    fn load_token_errors_when_absent() {
        with_temp_config(|| {
            assert!(load_token().is_err());
        });
    }

    #[test]
    fn clear_credentials_removes_token() {
        with_temp_config(|| {
            store_token("dtl_live_secret").unwrap();
            clear_credentials().unwrap();
            assert!(load_token().is_err());
        });
    }

    // ── update lock ──────────────────────────────────────────────────

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

    // ── comment preservation ─────────────────────────────────────────

    #[test]
    fn update_config_preserves_comments_and_unknown_keys() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            let seeded = "\
# Detail CLI config
# hand-edited — do not overwrite comments.

# API endpoint override
api_url = \"https://custom.example.com\"

# Auth token
api_token = \"old_token\"

# Unknown key the user added
custom_note = \"leave me alone\"
";
            fs::write(&path, seeded).unwrap();

            update_config(|config| {
                config.api_token = Some("new_token".into());
            })
            .unwrap();

            let raw = fs::read_to_string(&path).unwrap();
            assert!(
                raw.contains("# Detail CLI config"),
                "header comment lost:\n{raw}"
            );
            assert!(
                raw.contains("# API endpoint override"),
                "key comment lost:\n{raw}"
            );
            assert!(raw.contains("# Auth token"), "key comment lost:\n{raw}");
            assert!(
                raw.contains("# Unknown key the user added"),
                "unknown-key comment lost:\n{raw}"
            );
            assert!(raw.contains("custom_note"), "unknown key dropped:\n{raw}");
            assert!(raw.contains("\"new_token\""), "update did not land:\n{raw}");
            assert!(!raw.contains("old_token"), "old value lingered:\n{raw}");
        });
    }

    #[test]
    fn update_config_leaves_unchanged_keys_byte_identical() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            // Trailing inline comment + literal-string quoting are the kinds
            // of value-level decor that blanket-overwrite would clobber.
            let seeded = "\
api_url = 'https://api.staging.detail.dev' # staging override
api_token = \"old_token\"
";
            fs::write(&path, seeded).unwrap();

            update_config(|config| {
                config.api_token = Some("new_token".into());
            })
            .unwrap();

            let raw = fs::read_to_string(&path).unwrap();
            assert!(
                raw.contains("'https://api.staging.detail.dev' # staging override"),
                "untouched key's inline comment + quoting must survive verbatim:\n{raw}"
            );
            assert!(raw.contains("\"new_token\""), "update did not land:\n{raw}");
        });
    }

    #[test]
    fn update_config_on_empty_file_writes_mutated_fields() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            // File is created empty by update_config's `create(true)` + first
            // read of an empty handle returns "". This locks in that neither
            // the `DocumentMut` parse nor the `toml::from_str` deserialization
            // chokes on an empty string — the defensive `if contents.is_empty()`
            // branch we removed was never actually necessary.
            assert!(!path.exists(), "precondition: no config file yet");

            update_config(|config| {
                config.api_token = Some("fresh_token".into());
            })
            .unwrap();

            let raw = fs::read_to_string(&path).unwrap();
            assert!(
                raw.contains("api_token = \"fresh_token\""),
                "mutation missing:\n{raw}"
            );

            let loaded = load_config().unwrap();
            assert_eq!(loaded.api_token.as_deref(), Some("fresh_token"));
        });
    }

    #[test]
    fn update_config_removes_key_when_field_set_to_none() {
        with_temp_config(|| {
            let path = config_path().unwrap();
            fs::write(&path, "# token below\napi_token = \"to_be_cleared\"\n").unwrap();

            update_config(|config| {
                config.api_token = None;
            })
            .unwrap();

            let raw = fs::read_to_string(&path).unwrap();
            assert!(
                !raw.contains("api_token"),
                "cleared field still present:\n{raw}"
            );
        });
    }

    // ── concurrency ─────────────────────────────────────────────────

    #[test]
    fn concurrent_update_config_does_not_corrupt() {
        use std::thread;

        with_temp_config(|| {
            // Initialize config
            save_config(&Config::default()).unwrap();

            let handles: Vec<_> = (0..10)
                .map(|i| {
                    thread::spawn(move || {
                        // Each thread needs its own XDG_CONFIG_HOME since env vars are process-global.
                        // Since with_temp_config already set it, spawned threads inherit it.
                        update_config(|config| {
                            config.last_update_check = Some(i as u64);
                        })
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap().unwrap();
            }

            // Config should be valid TOML and parseable
            let config = load_config().unwrap();
            // last_update_check should be one of the values written (we don't know which due to races)
            assert!(config.last_update_check.is_some());
            // Most importantly: the file is not corrupted
            assert!(config.check_for_updates); // default value should survive
        });
    }

    /// Hammer concurrent readers against a writer. Pre-fix this race produced
    /// "Failed to parse config" errors at ~0.005% per read because
    /// `update_config` truncated the file in place; with the temp-file +
    /// rename strategy readers should never see a partial file.
    #[test]
    fn concurrent_reads_during_writes_never_observe_partial_toml() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        with_temp_config(|| {
            // Seed with a sizeable, valid config. The padded token pushes the
            // file across kernel page boundaries: pre-fix that's what makes
            // the race window wide enough for a reader's `read_to_end` to
            // observe a truncated file mid-rewrite within reasonable wall-
            // clock time. With ~80 byte writes the failure window in the
            // unfixed code was tight enough that a 30s test could miss it
            // entirely; with a 16 KiB rewrite it reproduces in seconds.
            let token_pad = "a".repeat(16 * 1024);
            update_config(|cfg| {
                cfg.api_token = Some(token_pad);
                cfg.api_url = Some("https://api.example.com".into());
            })
            .unwrap();

            let stop = Arc::new(AtomicBool::new(false));
            let reads = Arc::new(AtomicUsize::new(0));
            let writes = Arc::new(AtomicUsize::new(0));
            let read_errors = Arc::new(AtomicUsize::new(0));

            let mut readers = Vec::new();
            for _ in 0..4 {
                let stop = Arc::clone(&stop);
                let reads = Arc::clone(&reads);
                let read_errors = Arc::clone(&read_errors);
                readers.push(thread::spawn(move || -> Vec<String> {
                    let mut errs = Vec::new();
                    while !stop.load(Ordering::Relaxed) {
                        match load_config() {
                            Ok(_) => {}
                            Err(e) => {
                                read_errors.fetch_add(1, Ordering::Relaxed);
                                if errs.len() < 5 {
                                    errs.push(format!("{e:#}"));
                                }
                            }
                        }
                        reads.fetch_add(1, Ordering::Relaxed);
                    }
                    errs
                }));
            }

            let writer = {
                let stop = Arc::clone(&stop);
                let writes = Arc::clone(&writes);
                thread::spawn(move || {
                    let mut i: u64 = 0;
                    while !stop.load(Ordering::Relaxed) {
                        update_config(|cfg| {
                            cfg.last_update_check = Some(i);
                        })
                        .unwrap();
                        writes.fetch_add(1, Ordering::Relaxed);
                        i = i.wrapping_add(1);
                    }
                })
            };

            // Locally the unfixed code reliably produces parse errors well
            // before this budget is consumed. The 30s cap is a safety net
            // for slow CI; with the fix we typically hit the write target
            // in a few seconds and exit early.
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if writes.load(Ordering::Relaxed) >= 3_000 {
                    break;
                }
                if Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            stop.store(true, Ordering::Relaxed);
            writer.join().unwrap();
            let mut sample_errors: Vec<String> = Vec::new();
            for h in readers {
                sample_errors.extend(h.join().unwrap());
            }

            let total_reads = reads.load(Ordering::Relaxed);
            let total_writes = writes.load(Ordering::Relaxed);
            let errs = read_errors.load(Ordering::Relaxed);
            assert_eq!(
                errs, 0,
                "saw {errs} read errors across {total_reads} reads / {total_writes} writes; first samples: {sample_errors:#?}",
            );
        });
    }
}
