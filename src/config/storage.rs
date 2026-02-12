use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
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
    let config_dir = if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg_config).join("detail-cli")
    } else if cfg!(windows) {
        // Windows: use LOCALAPPDATA
        let local_app_data =
            std::env::var("LOCALAPPDATA").context("LOCALAPPDATA environment variable not set")?;
        PathBuf::from(local_app_data).join("detail-cli")
    } else {
        // Others: use ~/.config
        let home = homedir::my_home()
            .context("Failed to determine home directory")?
            .context("Home directory not found")?;
        home.join(".config").join("detail-cli")
    };

    std::fs::create_dir_all(&config_dir)?;
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

    let contents = std::fs::read_to_string(path)?;
    toml::from_str(&contents).context("Failed to parse config")
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    let contents = toml::to_string_pretty(config)?;
    std::fs::write(path, contents)?;
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
