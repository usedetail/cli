use std::path::PathBuf;
use std::{env, fs};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub api_url: Option<String>,
    pub check_for_updates: bool,
    pub last_update_check: Option<u64>,
    pub api_token: Option<String>,
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
        return Ok(Config {
            check_for_updates: true,
            ..Default::default()
        });
    }

    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents).context("Failed to parse config")
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    let contents = toml::to_string_pretty(config)?;
    write_private(path, contents.as_bytes())?;
    Ok(())
}

/// Write `data` to `path`, restricting the file to owner-only access (0600) on Unix.
fn write_private(path: PathBuf, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, data)?;
    }
    Ok(())
}

// Token storage in config file
pub fn store_token(token: &str) -> Result<()> {
    let mut config = load_config()?;
    config.api_token = Some(token.to_string());
    save_config(&config)?;
    Ok(())
}

pub fn load_token() -> Result<String> {
    let config = load_config()?;
    config
        .api_token
        .context("No token found. Run `detail auth login`")
}

pub fn clear_credentials() -> Result<()> {
    let mut config = load_config()?;
    config.api_token = None;
    save_config(&config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Mutex to serialize tests that modify the XDG_CONFIG_HOME env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Run a closure with XDG_CONFIG_HOME pointing to a fresh temp directory,
    /// restoring the original value afterwards.
    fn with_temp_config<F: FnOnce() -> R, R>(f: F) -> R {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = env::temp_dir().join(format!("detail-cli-test-{}", std::process::id()));
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
        assert!(toml_str.contains("check_for_updates = false"));
    }

    #[test]
    fn config_round_trip_via_toml() {
        let config = Config {
            api_url: Some("https://api.example.com".into()),
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
    fn config_missing_all_fields_uses_defaults() {
        // Simulates an old config.toml that predates check_for_updates
        let toml_str = "api_token = \"dtl_old\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.check_for_updates); // Default::default() for bool
        assert_eq!(config.api_token.as_deref(), Some("dtl_old"));
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
    fn load_config_returns_defaults_when_no_file() {
        with_temp_config(|| {
            let config = load_config().unwrap();
            assert!(config.check_for_updates);
            assert!(config.api_token.is_none());
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

    #[cfg(unix)]
    #[test]
    fn config_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        with_temp_config(|| {
            store_token("dtl_live_secret").unwrap();
            let path = config_path().unwrap();
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "config file should be owner-only (0600)");
        });
    }
}
