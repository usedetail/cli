use std::env;
use std::io::{self, Write};

use anyhow::{bail, Context, Result};

fn detect_shell() -> Result<String> {
    let shell = env::var("SHELL").context("could not detect shell from $SHELL")?;
    let shell = shell.trim();
    if shell.is_empty() {
        bail!("$SHELL is empty");
    }
    let shell = shell.trim_end_matches('/');
    // $SHELL is e.g. "/bin/zsh" — take the basename
    let name = shell.rsplit('/').next().unwrap_or(shell);
    if name.is_empty() {
        bail!("could not extract shell name from $SHELL: {shell}");
    }
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

fn print_snippet<W: Write>(shell: &str, out: &mut W) -> Result<()> {
    writeln!(out, "{}", snippet(shell)?)?;
    Ok(())
}

pub fn handle(shell: Option<&str>) -> Result<()> {
    let detected;
    let shell = if let Some(s) = shell {
        s
    } else {
        detected = detect_shell()?;
        detected.as_str()
    };
    let stdout = io::stdout();
    print_snippet(shell, &mut stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static SHELL_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_shell_var(value: &str, f: impl FnOnce()) {
        let _guard = SHELL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = env::var("SHELL").ok();
        env::set_var("SHELL", value);
        f();
        if let Some(val) = original {
            env::set_var("SHELL", val);
        } else {
            env::remove_var("SHELL");
        }
    }

    #[test]
    fn snippet_bash() {
        assert!(snippet("bash").unwrap().contains("COMPLETE=bash"));
    }

    #[test]
    fn snippet_zsh() {
        assert!(snippet("zsh").unwrap().contains("COMPLETE=zsh"));
    }

    #[test]
    fn snippet_fish() {
        assert!(snippet("fish").unwrap().contains("COMPLETE=fish"));
    }

    #[test]
    fn snippet_elvish() {
        assert!(snippet("elvish").unwrap().contains("COMPLETE=elvish"));
    }

    #[test]
    fn snippet_powershell() {
        assert!(snippet("powershell").unwrap().contains("COMPLETE"));
    }

    #[test]
    fn snippet_pwsh_matches_powershell() {
        assert_eq!(snippet("pwsh").unwrap(), snippet("powershell").unwrap());
    }

    #[test]
    fn snippet_unsupported_shell_errors() {
        assert!(snippet("tcsh").is_err());
    }

    #[test]
    fn detect_shell_from_env() {
        with_shell_var("/usr/bin/zsh", || {
            assert_eq!(detect_shell().unwrap(), "zsh");
        });
        with_shell_var("/bin/bash", || {
            assert_eq!(detect_shell().unwrap(), "bash");
        });
        with_shell_var("fish", || {
            assert_eq!(detect_shell().unwrap(), "fish");
        });
    }

    #[test]
    fn print_snippet_writes_to_writer() {
        let mut out = Vec::new();
        print_snippet("bash", &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "eval \"$(COMPLETE=bash detail 2>/dev/null)\"\n");
    }

    #[test]
    fn print_snippet_unsupported_shell_errors() {
        let mut out = Vec::new();
        assert!(print_snippet("tcsh", &mut out).is_err());
    }

    #[test]
    fn detect_shell_empty_errors() {
        with_shell_var("", || {
            let err = detect_shell().unwrap_err();
            assert!(
                err.to_string().contains("$SHELL is empty"),
                "unexpected error: {err}"
            );
        });
    }

    #[test]
    fn detect_shell_trailing_slashes() {
        with_shell_var("/usr/bin/zsh/", || {
            assert_eq!(detect_shell().unwrap(), "zsh");
        });
        with_shell_var("/usr/bin/bash///", || {
            assert_eq!(detect_shell().unwrap(), "bash");
        });
    }

    #[test]
    fn detect_shell_whitespace_only_errors() {
        with_shell_var("   ", || {
            let err = detect_shell().unwrap_err();
            assert!(
                err.to_string().contains("$SHELL is empty"),
                "unexpected error: {err}"
            );
        });
    }
}
