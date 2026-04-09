use anyhow::{bail, Context, Result};
use std::process::Command;

/// Extract `owner/repo` from a GitHub remote URL.
///
/// Supports HTTPS (`https://github.com/owner/repo.git`),
/// SSH colon (`git@github.com:owner/repo.git`), and
/// SSH scheme (`ssh://git@github.com/owner/repo.git`) formats.
fn parse_github_remote_url(url: &str) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ];

    let rest = PREFIXES.iter().find_map(|p| url.strip_prefix(p))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    (parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty())
        .then(|| format!("{}/{}", parts[0], parts[1]))
}

/// Get the URL for a named git remote, or `None` if it doesn't exist.
fn get_remote_url(remote: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", remote])
        .output()
        .ok()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if url.is_empty() {
            None
        } else {
            Some(url)
        }
    } else {
        None
    }
}

/// Infer the `owner/repo` identifier from the current git repository by
/// checking `upstream` first, then `origin`.
///
/// Returns `Ok(owner/repo)` on success, or an error if we are not inside a
/// git repository or neither remote points to a recognisable GitHub URL.
pub fn infer_repo_from_git_remote() -> Result<String> {
    // Make sure we are inside a git work-tree.
    let in_git = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("Failed to run git")?;

    if !in_git.status.success() {
        bail!(
            "Not inside a git repository. Please pass a repo argument explicitly \
             (e.g. owner/repo)."
        );
    }

    // Try upstream first, then origin.
    for remote in &["upstream", "origin"] {
        if let Some(url) = get_remote_url(remote) {
            if let Some(owner_repo) = parse_github_remote_url(&url) {
                return Ok(owner_repo);
            }
        }
    }

    bail!(
        "Could not infer repository from git remotes. \
         Please pass a repo argument explicitly (e.g. owner/repo)."
    )
}

/// If `explicit` is `Some`, return it. Otherwise try to infer from the git
/// remote. Wraps the inference error to tell the user to supply the argument.
pub fn resolve_repo_arg(explicit: Option<&str>) -> Result<String> {
    explicit.map_or_else(infer_repo_from_git_remote, |r| Ok(r.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_github_remote_url ─────────────────────────────────────

    #[test]
    fn parses_https_with_git_suffix() {
        assert_eq!(
            parse_github_remote_url("https://github.com/usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_https_without_git_suffix() {
        assert_eq!(
            parse_github_remote_url("https://github.com/usedetail/cli"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_https_with_trailing_slash() {
        assert_eq!(
            parse_github_remote_url("https://github.com/usedetail/cli/"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_colon_format() {
        assert_eq!(
            parse_github_remote_url("git@github.com:usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_colon_format_no_suffix() {
        assert_eq!(
            parse_github_remote_url("git@github.com:usedetail/cli"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_scheme_format() {
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com/usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn returns_none_for_non_github_url() {
        assert_eq!(
            parse_github_remote_url("https://gitlab.com/usedetail/cli.git"),
            None,
        );
    }

    #[test]
    fn returns_none_for_empty_string() {
        assert_eq!(parse_github_remote_url(""), None);
    }

    #[test]
    fn returns_none_for_malformed_url() {
        assert_eq!(parse_github_remote_url("not-a-url"), None);
    }

    #[test]
    fn parses_http_url() {
        assert_eq!(
            parse_github_remote_url("http://github.com/owner/repo.git"),
            Some("owner/repo".to_string()),
        );
    }

    #[test]
    fn parses_https_with_git_suffix_and_trailing_slash() {
        assert_eq!(
            parse_github_remote_url("https://github.com/usedetail/cli.git/"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_colon_with_git_suffix_and_trailing_slash() {
        assert_eq!(
            parse_github_remote_url("git@github.com:usedetail/cli.git/"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_scheme_with_git_suffix_and_trailing_slash() {
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com/usedetail/cli.git/"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn ignores_extra_path_segments() {
        // e.g. https://github.com/owner/repo/tree/main — should still extract owner/repo
        assert_eq!(
            parse_github_remote_url("https://github.com/owner/repo/tree/main"),
            Some("owner/repo".to_string()),
        );
    }

    // ── resolve_repo_arg ────────────────────────────────────────────

    #[test]
    fn resolve_explicit_returns_value() {
        assert_eq!(
            resolve_repo_arg(Some("usedetail/cli")).ok(),
            Some("usedetail/cli".to_string()),
        );
    }
}
