use anyhow::{bail, Result};
use std::process::Command;

/// Extract `owner/repo` from a GitHub remote URL.
///
/// Supports HTTPS (`https://github.com/owner/repo.git`),
/// SSH colon (`git@github.com:owner/repo.git`), and
/// SSH scheme (`ssh://git@github.com[:PORT]/owner/repo.git`) formats.
fn parse_github_remote_url(url: &str) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ];

    // Git accepts HTTPS remotes with embedded credentials
    // (https://token@github.com/…, https://user:pass@github.com/…) and SSH
    // remotes with explicit ports (ssh://git@github.com:22/…). Normalize
    // either to a canonical form before prefix matching.
    let normalized = strip_http_credentials(url).or_else(|| strip_ssh_port(url));
    let url = normalized.as_deref().unwrap_or(url);

    let rest = PREFIXES.iter().find_map(|p| url.strip_prefix(p))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    (parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty())
        .then(|| format!("{}/{}", parts[0], parts[1]))
}

/// If `url` is an `ssh://` URL whose authority includes an explicit port
/// (e.g. `ssh://git@github.com:22/owner/repo.git`), return the URL with the
/// port stripped. Returns `None` for SSH URLs without a port or for URLs
/// that don't use the `ssh://` scheme.
fn strip_ssh_port(url: &str) -> Option<String> {
    let rest = url.strip_prefix("ssh://")?;
    // The authority runs up to the first `/`. Anything after stays as-is,
    // so a `:` later in the path won't be mistaken for a port separator.
    let (authority, path) = rest.split_once('/')?;
    // Authority is `[user@]host[:port]`. Split off the optional `user@`.
    let (user_at, host_port) = match authority.rsplit_once('@') {
        Some((u, h)) => (format!("{u}@"), h),
        None => (String::new(), authority),
    };
    // Only normalize when there's actually a port to strip.
    let host = if host_port.starts_with('[') {
        // IPv6 bracketed host, e.g. `[::1]:22` — split on `]:`.
        host_port.split_once("]:").map(|(h, _)| format!("{h}]"))?
    } else {
        host_port.rsplit_once(':').map(|(h, _)| h.to_string())?
    };
    Some(format!("ssh://{user_at}{host}/{path}"))
}

/// If `url` is an http(s) URL with `user[:pass]@` credentials in the authority,
/// return the URL with those credentials stripped. Returns `None` for URLs
/// without credentials or with non-http(s) schemes.
fn strip_http_credentials(url: &str) -> Option<String> {
    for scheme in ["https://", "http://"] {
        if let Some(rest) = url.strip_prefix(scheme) {
            // Credentials live in the authority, which runs up to the first `/`
            // (or end of string). Anything after the authority — including a
            // `@` in the path — stays untouched.
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            if let Some((_, host)) = authority.split_once('@') {
                return Some(format!("{scheme}{host}/{path}"));
            }
        }
    }
    None
}

/// Infer the `owner/repo` identifier from the current git repository by
/// checking the `origin` remote.
///
/// Returns `Ok(owner/repo)` on success, or an error if we are not inside a
/// git repository or the `origin` remote is not a recognisable GitHub URL.
pub fn infer_repo_from_git_remote() -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
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

    #[test]
    fn parses_https_with_token_credentials() {
        assert_eq!(
            parse_github_remote_url("https://token@github.com/usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_https_with_user_pass_credentials() {
        assert_eq!(
            parse_github_remote_url("https://user:pass@github.com/usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_http_with_credentials() {
        assert_eq!(
            parse_github_remote_url("http://user:pass@github.com/owner/repo"),
            Some("owner/repo".to_string()),
        );
    }

    #[test]
    fn leaves_at_in_path_untouched() {
        // An `@` after the first `/` is part of the path, not credentials.
        assert_eq!(
            parse_github_remote_url("https://github.com/owner/repo@tag"),
            // Trailing `@tag` is treated as part of the repo name; gets trimmed
            // along with anything after owner/repo.
            Some("owner/repo@tag".to_string()),
        );
    }

    #[test]
    fn rejects_non_github_host_even_with_credentials() {
        assert_eq!(
            parse_github_remote_url("https://token@gitlab.com/owner/repo.git"),
            None,
        );
    }

    #[test]
    fn parses_ssh_scheme_with_port() {
        // Git accepts this URL format; the CLI should too.
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com:22/owner/repo.git"),
            Some("owner/repo".to_string()),
        );
    }

    #[test]
    fn parses_ssh_scheme_with_nondefault_port() {
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com:443/usedetail/cli.git"),
            Some("usedetail/cli".to_string()),
        );
    }

    #[test]
    fn parses_ssh_scheme_with_port_no_git_suffix() {
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com:22/owner/repo"),
            Some("owner/repo".to_string()),
        );
    }

    #[test]
    fn parses_ssh_scheme_with_port_and_trailing_slash() {
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com:22/owner/repo.git/"),
            Some("owner/repo".to_string()),
        );
    }

    #[test]
    fn strip_ssh_port_handles_ipv6() {
        let result = strip_ssh_port("ssh://git@[::1]:22/owner/repo.git");
        assert_eq!(result, Some("ssh://git@[::1]/owner/repo.git".to_string()),);
    }

    #[test]
    fn rejects_ssh_scheme_with_port_for_non_github_host() {
        assert_eq!(
            parse_github_remote_url("ssh://git@gitlab.com:22/owner/repo.git"),
            None,
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
