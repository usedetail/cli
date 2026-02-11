use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SERVICE_NAME: &str = "com.detail.cli";
const TOKEN_KEY: &str = "api_token";

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub api_url: Option<String>,
    pub check_for_updates: bool,
    pub last_update_check: Option<u64>,
}

pub fn config_path() -> Result<PathBuf> {
    let config_dir = directories::ProjectDirs::from("com", "detail", "cli")
        .context("Failed to determine config directory")?;

    std::fs::create_dir_all(config_dir.config_dir())?;
    Ok(config_dir.config_dir().join("config.toml"))
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

// Token storage using system keychain
pub fn store_token(token: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)?;
    entry.set_password(token)?;
    Ok(())
}

pub fn load_token() -> Result<String> {
    let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)?;
    entry
        .get_password()
        .context("No token found. Run `detail auth login`")
}

pub fn clear_credentials() -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, TOKEN_KEY)?;
    entry.delete_credential().ok(); // Ignore errors if already deleted
    Ok(())
}
