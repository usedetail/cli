use std::fs::File;
use std::io::{ErrorKind, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::PathBuf;
use std::{env, fs};

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

/// Atomically read-modify-write the config file under an exclusive lock.
///
/// Preserves comments and formatting the user may have added by hand. The
/// file is parsed as a `toml_edit::DocumentMut`; we snapshot the typed
/// `Config` before and after the closure and only touch keys whose value
/// actually changed. That keeps line comments above each key, trailing
/// inline comments, and the user's chosen quote style intact for fields
/// the update didn't care about — and leaves unknown user-added keys alone.
pub fn update_config(f: impl FnOnce(&mut Config)) -> Result<()> {
    let path = config_path()?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.lock_exclusive()?;

    let mut contents = String::new();
    (&file).read_to_string(&mut contents)?;

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

    let new_contents = doc.to_string();
    // Rewrite through the same handle. `File::create` would open a second
    // file description and leave `file.unlock()` acting on a handle that was
    // never locked — functionally OK because the real unlock still happens
    // via Drop at end of scope, but confusing to read.
    (&file).seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    (&file).write_all(new_contents.as_bytes())?;
    file.unlock()?;
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
    use std::thread;

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

    // ── load_config ──────────────────────────────────────────────────

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
        with_temp_config(|| {
            // Initialize config
            update_config(|_| {}).unwrap();

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
}
