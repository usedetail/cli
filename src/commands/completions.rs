use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

fn detect_shell() -> Result<String> {
    let shell = env::var("SHELL").context("could not detect shell from $SHELL")?;
    // $SHELL is e.g. "/bin/zsh" — take the basename
    let name = shell.rsplit('/').next().unwrap_or(&shell);
    Ok(name.to_lowercase())
}

fn snippet(shell: &str) -> Result<&'static str> {
    match shell {
        "bash" => Ok("eval \"$(COMPLETE=bash detail 2>/dev/null)\""),
        "zsh" => Ok("source <(COMPLETE=zsh detail)"),
        "fish" => Ok("COMPLETE=fish detail | source"),
        "elvish" => Ok("eval (E:COMPLETE=elvish detail | slurp)"),
        "powershell" | "pwsh" => Ok(
            "$env:COMPLETE = \"powershell\"; detail | Out-String | Invoke-Expression; Remove-Item Env:\\COMPLETE",
        ),
        _ => bail!("unsupported shell: {shell} (supported: bash, zsh, fish, elvish, powershell)"),
    }
}

/// Bash reads different rc files for login vs. non-login shells, so install
/// the snippet into every existing one (mirroring rustup's behavior). If none
/// exist, create `.bashrc`.
fn bash_rc_paths(home: &Path) -> Vec<PathBuf> {
    let candidates = [".bash_profile", ".bash_login", ".bashrc"];
    let existing: Vec<PathBuf> = candidates
        .iter()
        .map(|name| home.join(name))
        .filter(|p| p.exists())
        .collect();
    if existing.is_empty() {
        vec![home.join(".bashrc")]
    } else {
        existing
    }
}

fn rc_paths(shell: &str) -> Result<Vec<PathBuf>> {
    let home = homedir::my_home()?.context("could not determine home directory")?;
    match shell {
        "bash" => Ok(bash_rc_paths(&home)),
        "zsh" => Ok(vec![home.join(".zshrc")]),
        "fish" => Ok(vec![home.join(".config/fish/completions/detail.fish")]),
        "elvish" => {
            let config_dir =
                env::var("XDG_CONFIG_HOME").map_or_else(|_| home.join(".config"), PathBuf::from);
            Ok(vec![config_dir.join("elvish/rc.elv")])
        }
        "powershell" | "pwsh" => {
            Ok(vec![home.join(
                ".config/powershell/Microsoft.PowerShell_profile.ps1",
            )])
        }
        _ => bail!("unsupported shell: {shell}"),
    }
}

/// Append the snippet to `rc` if it isn't already present. Returns `true` if
/// the file was modified, `false` if the snippet was already installed.
fn install_snippet(rc: &Path, snippet: &str) -> Result<bool> {
    if rc.exists() {
        let contents = fs::read_to_string(rc)?;
        if contents.contains(snippet) {
            return Ok(false);
        }
    }

    if let Some(parent) = rc.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new().create(true).append(true).open(rc)?;
    writeln!(file)?;
    writeln!(file, "# Detail CLI shell completions")?;
    writeln!(file, "{snippet}")?;
    Ok(true)
}

pub fn handle() -> Result<()> {
    let shell = detect_shell()?;
    let snippet = snippet(&shell)?;
    let rcs = rc_paths(&shell)?;

    let mut installed = Vec::new();
    let mut already = Vec::new();
    for rc in &rcs {
        if install_snippet(rc, snippet)? {
            installed.push(rc.clone());
        } else {
            already.push(rc.clone());
        }
    }

    let term = console::Term::stderr();
    let join = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    if installed.is_empty() {
        term.write_line(&format!(
            "Completions already installed in {}",
            join(&already),
        ))?;
    } else {
        term.write_line(&format!(
            "Installed completions in {} — restart your shell or run:\n  {snippet}",
            join(&installed),
        ))?;
        if !already.is_empty() {
            term.write_line(&format!("(already present in {})", join(&already),))?;
        }
    }
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
    fn rc_paths_zsh_is_zshrc() {
        let rc = rc_paths("zsh").unwrap();
        assert_eq!(rc.len(), 1);
        assert!(rc[0].ends_with(".zshrc"));
    }

    #[test]
    fn rc_paths_fish_is_completions_dir() {
        let rc = rc_paths("fish").unwrap();
        assert_eq!(rc.len(), 1);
        assert!(rc[0].ends_with("fish/completions/detail.fish"));
    }

    #[test]
    fn rc_paths_elvish_is_rc_elv() {
        let rc = rc_paths("elvish").unwrap();
        assert_eq!(rc.len(), 1);
        assert!(
            rc[0].ends_with(".config/elvish/rc.elv"),
            "expected XDG-compliant elvish path, got: {}",
            rc[0].display()
        );
    }

    #[test]
    fn rc_paths_unsupported_errors() {
        assert!(rc_paths("tcsh").is_err());
    }

    #[test]
    fn rc_paths_powershell_and_pwsh_equivalent() {
        let ps = rc_paths("powershell").unwrap();
        let pwsh = rc_paths("pwsh").unwrap();
        assert_eq!(ps, pwsh);
    }

    #[test]
    fn rc_paths_powershell_ignores_profile_env_var() {
        // PROFILE is a Windows system env var (user profile dir), NOT the
        // PowerShell $PROFILE automatic variable. It must not affect the path.
        let original = env::var("PROFILE").ok();
        env::set_var("PROFILE", "/wrong/path");
        let rc = rc_paths("powershell").unwrap();
        assert!(
            rc[0].ends_with("powershell/Microsoft.PowerShell_profile.ps1"),
            "PROFILE env var should not affect PowerShell rc path, got: {}",
            rc[0].display()
        );
        match original {
            Some(v) => env::set_var("PROFILE", v),
            None => env::remove_var("PROFILE"),
        }
    }

    #[test]
    fn rc_paths_elvish_respects_xdg_config_home() {
        let original = env::var("XDG_CONFIG_HOME").ok();
        env::set_var("XDG_CONFIG_HOME", "/custom/config");
        let rc = rc_paths("elvish").unwrap();
        assert_eq!(
            rc,
            vec![PathBuf::from("/custom/config/elvish/rc.elv")],
            "elvish should respect XDG_CONFIG_HOME"
        );
        match original {
            Some(v) => env::set_var("XDG_CONFIG_HOME", v),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    // ── bash_rc_paths ────────────────────────────────────────────────

    fn fresh_home(label: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "detail-test-bash-{}-{label}-{}",
            std::process::id(),
            // unique per test invocation
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bash_rc_paths_only_bashrc() {
        let home = fresh_home("only-bashrc");
        fs::write(home.join(".bashrc"), "").unwrap();

        let rcs = bash_rc_paths(&home);
        assert_eq!(rcs, vec![home.join(".bashrc")]);

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn bash_rc_paths_only_bash_profile() {
        let home = fresh_home("only-bash-profile");
        fs::write(home.join(".bash_profile"), "").unwrap();

        let rcs = bash_rc_paths(&home);
        assert_eq!(rcs, vec![home.join(".bash_profile")]);

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn bash_rc_paths_both_exist() {
        let home = fresh_home("both");
        fs::write(home.join(".bashrc"), "").unwrap();
        fs::write(home.join(".bash_profile"), "").unwrap();

        let rcs = bash_rc_paths(&home);
        assert_eq!(
            rcs,
            vec![home.join(".bash_profile"), home.join(".bashrc")],
            "expected both .bash_profile and .bashrc, got {rcs:?}"
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn bash_rc_paths_all_three_exist() {
        let home = fresh_home("all-three");
        fs::write(home.join(".bash_profile"), "").unwrap();
        fs::write(home.join(".bash_login"), "").unwrap();
        fs::write(home.join(".bashrc"), "").unwrap();

        let rcs = bash_rc_paths(&home);
        assert_eq!(
            rcs,
            vec![
                home.join(".bash_profile"),
                home.join(".bash_login"),
                home.join(".bashrc"),
            ],
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn bash_rc_paths_neither_exists_creates_bashrc() {
        let home = fresh_home("neither");

        let rcs = bash_rc_paths(&home);
        assert_eq!(rcs, vec![home.join(".bashrc")]);
        // The function returns the path but should not have created the file.
        assert!(!home.join(".bashrc").exists());

        let _ = fs::remove_dir_all(&home);
    }

    // ── install_snippet ──────────────────────────────────────────────

    #[test]
    fn install_snippet_appends_to_new_file() {
        let home = fresh_home("install-new");
        let rc = home.join(".bashrc");

        let modified = install_snippet(&rc, "SNIPPET").unwrap();
        assert!(modified);
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("SNIPPET"));
        assert!(contents.contains("# Detail CLI shell completions"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn install_snippet_preserves_existing_content() {
        let home = fresh_home("install-existing");
        let rc = home.join(".bashrc");
        fs::write(&rc, "export FOO=bar\n").unwrap();

        let modified = install_snippet(&rc, "SNIPPET").unwrap();
        assert!(modified);
        let contents = fs::read_to_string(&rc).unwrap();
        assert!(contents.starts_with("export FOO=bar\n"));
        assert!(contents.contains("SNIPPET"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn install_snippet_is_idempotent() {
        let home = fresh_home("install-idempotent");
        let rc = home.join(".bashrc");

        assert!(install_snippet(&rc, "SNIPPET").unwrap());
        let after_first = fs::read_to_string(&rc).unwrap();

        // Second call must report "not modified" and leave the file alone.
        assert!(!install_snippet(&rc, "SNIPPET").unwrap());
        let after_second = fs::read_to_string(&rc).unwrap();
        assert_eq!(after_first, after_second);

        // A third call after appending unrelated content also stays idempotent.
        fs::OpenOptions::new()
            .append(true)
            .open(&rc)
            .unwrap()
            .write_all(b"\nexport BAZ=1\n")
            .unwrap();
        assert!(!install_snippet(&rc, "SNIPPET").unwrap());

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn install_snippet_per_file_idempotency_across_bash_rcs() {
        // If one bash rc already has the snippet but another doesn't, only the
        // missing one should be modified.
        let home = fresh_home("install-cross");
        let bashrc = home.join(".bashrc");
        let profile = home.join(".bash_profile");
        fs::write(&bashrc, "SNIPPET\n").unwrap();
        fs::write(&profile, "").unwrap();

        assert!(!install_snippet(&bashrc, "SNIPPET").unwrap());
        assert!(install_snippet(&profile, "SNIPPET").unwrap());
        assert!(fs::read_to_string(&profile).unwrap().contains("SNIPPET"));

        let _ = fs::remove_dir_all(&home);
    }
}
