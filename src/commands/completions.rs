use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

fn detect_shell() -> Result<String> {
    let shell = env::var("SHELL").context("could not detect shell from $SHELL")?;
    // $SHELL is e.g. "/bin/zsh" — take the basename
    let name = shell.rsplit('/').next().unwrap_or(&shell);
    Ok(name.to_lowercase())
}

fn snippet(shell: &str) -> Result<&'static str> {
    match shell {
        "bash" => Ok("source <(COMPLETE=bash detail)"),
        "zsh" => Ok("source <(COMPLETE=zsh detail)"),
        "fish" => Ok("COMPLETE=fish detail | source"),
        "elvish" => Ok("eval (E:COMPLETE=elvish detail | slurp)"),
        "powershell" | "pwsh" => Ok(
            "$env:COMPLETE = \"powershell\"; detail | Out-String | Invoke-Expression; Remove-Item Env:\\COMPLETE",
        ),
        _ => bail!("unsupported shell: {shell} (supported: bash, zsh, fish, elvish, powershell)"),
    }
}

fn rc_path(shell: &str) -> Result<PathBuf> {
    let home = homedir::my_home()?.context("could not determine home directory")?;
    match shell {
        "bash" => {
            let bashrc = home.join(".bashrc");
            let profile = home.join(".bash_profile");
            // Prefer .bashrc if it exists, else .bash_profile, else create .bashrc
            if bashrc.exists() {
                Ok(bashrc)
            } else if profile.exists() {
                Ok(profile)
            } else {
                Ok(bashrc)
            }
        }
        "zsh" => Ok(home.join(".zshrc")),
        "fish" => Ok(home.join(".config/fish/completions/detail.fish")),
        "elvish" => Ok(home.join(".elvish/rc.elv")),
        "powershell" | "pwsh" => Ok(env::var("PROFILE").map_or_else(
            |_| home.join(".config/powershell/Microsoft.PowerShell_profile.ps1"),
            PathBuf::from,
        )),
        _ => bail!("unsupported shell: {shell}"),
    }
}

pub fn handle() -> Result<()> {
    let shell = detect_shell()?;
    let snippet = snippet(&shell)?;
    let rc = rc_path(&shell)?;

    // Check if already installed
    if rc.exists() {
        let contents = fs::read_to_string(&rc)?;
        if contents.contains(snippet) {
            console::Term::stderr().write_line(&format!(
                "Completions already installed in {}",
                rc.display(),
            ))?;
            return Ok(());
        }
    }

    // Ensure parent directory exists (relevant for fish/elvish/powershell)
    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc)?;
    writeln!(file)?;
    writeln!(file, "# Detail CLI shell completions")?;
    writeln!(file, "{snippet}")?;

    console::Term::stderr().write_line(&format!(
        "Installed completions in {} — restart your shell or run:\n  {snippet}",
        rc.display(),
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_bash() {
        assert!(snippet("bash").is_ok());
        assert!(snippet("bash").unwrap().contains("COMPLETE=bash"));
    }

    #[test]
    fn snippet_zsh() {
        assert!(snippet("zsh").is_ok());
        assert!(snippet("zsh").unwrap().contains("COMPLETE=zsh"));
    }

    #[test]
    fn snippet_fish() {
        assert!(snippet("fish").is_ok());
        assert!(snippet("fish").unwrap().contains("COMPLETE=fish"));
    }

    #[test]
    fn snippet_elvish() {
        assert!(snippet("elvish").is_ok());
        assert!(snippet("elvish").unwrap().contains("COMPLETE=elvish"));
    }

    #[test]
    fn snippet_powershell() {
        assert!(snippet("powershell").is_ok());
        assert!(snippet("powershell").unwrap().contains("COMPLETE"));
    }

    #[test]
    fn snippet_pwsh() {
        assert_eq!(snippet("pwsh").unwrap(), snippet("powershell").unwrap());
    }

    #[test]
    fn snippet_unsupported_shell_errors() {
        assert!(snippet("tcsh").is_err());
    }

    #[test]
    fn detect_shell_from_env() {
        // Save and restore $SHELL
        let original = env::var("SHELL").ok();
        env::set_var("SHELL", "/usr/bin/zsh");
        assert_eq!(detect_shell().unwrap(), "zsh");

        env::set_var("SHELL", "/bin/bash");
        assert_eq!(detect_shell().unwrap(), "bash");

        env::set_var("SHELL", "fish");
        assert_eq!(detect_shell().unwrap(), "fish");

        if let Some(val) = original {
            env::set_var("SHELL", val);
        }
    }

    #[test]
    fn rc_path_zsh_is_zshrc() {
        let rc = rc_path("zsh").unwrap();
        assert!(rc.ends_with(".zshrc"));
    }

    #[test]
    fn rc_path_fish_is_completions_dir() {
        let rc = rc_path("fish").unwrap();
        assert!(rc.ends_with("fish/completions/detail.fish"));
    }

    #[test]
    fn rc_path_elvish_is_rc_elv() {
        let rc = rc_path("elvish").unwrap();
        assert!(rc.ends_with(".elvish/rc.elv"));
    }

    #[test]
    fn rc_path_unsupported_errors() {
        assert!(rc_path("tcsh").is_err());
    }
}
