use anyhow::{bail, Context, Result};

use crate::api::client::ApiClient;
use crate::api::types::{Repo, RepoId};

/// Page size used when paginating through repos to resolve identifiers.
const REPO_PAGE_SIZE: u32 = 100;

/// Fetch all repos by paginating through the API.
pub async fn fetch_all_repos(client: &ApiClient) -> Result<Vec<Repo>> {
    let mut all_repos = Vec::new();
    let mut offset = 0;

    loop {
        let repos = client
            .list_repos(REPO_PAGE_SIZE, offset)
            .await
            .context("Failed to fetch repositories while resolving identifier")?;

        let total = usize::try_from(repos.total.max(0)).unwrap_or(0);
        let page_size = repos.repos.len();
        all_repos.extend(repos.repos);

        if page_size == 0 || all_repos.len() >= total {
            break;
        }
        offset += u32::try_from(page_size).unwrap_or(REPO_PAGE_SIZE);
    }

    Ok(all_repos)
}

/// Validate that a slash-containing identifier has exactly one slash with
/// non-empty owner and repo parts.
pub fn validate_owner_repo_format(identifier: &str) -> Result<()> {
    let parts: Vec<&str> = identifier.split('/').collect();
    if parts.len() != 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        bail!(
            "Invalid repository format. Please use owner/repo (e.g., 'usedetail/cli') or just the repo name. Run 'detail repos list' to see your repositories."
        );
    }
    Ok(())
}

/// Given a bare repo name and the full list of accessible repos, return the
/// matching repo ID — or a helpful error when zero or multiple repos match.
pub fn match_repo_by_name(name: &str, repos: &[Repo]) -> Result<RepoId> {
    let matching: Vec<_> = repos.iter().filter(|r| r.name == name).collect();

    match matching.len() {
        0 => bail!(
            "Repository '{name}' not found. Run 'detail repos list' to see your repositories."
        ),
        1 => Ok(matching[0].id.clone()),
        _ => {
            let repo_list: Vec<String> = matching
                .iter()
                .map(|r| format!("  - {}", r.full_name))
                .collect();
            bail!(
                "Multiple repositories with name '{}' found:\n{}\n\nPlease specify using owner/repo format (e.g., '{}').",
                name,
                repo_list.join("\n"),
                matching[0].full_name
            )
        }
    }
}

/// Resolve owner/repo or repo name to repo ID, searching across all accessible repos.
pub async fn resolve_repo_id(client: &ApiClient, repo_identifier: &str) -> Result<RepoId> {
    let repos = fetch_all_repos(client).await?;
    resolve_repo_id_from_repos(&repos, repo_identifier)
}

pub fn resolve_repo_id_from_repos(repos: &[Repo], repo_identifier: &str) -> Result<RepoId> {
    // Normalize accidental whitespace before matching. `validate_owner_repo_format`
    // already tolerated whitespace-only emptiness checks but the original input
    // was then compared verbatim against `r.full_name`, producing a misleading
    // "not found" error for inputs like " usedetail/cli".
    let identifier = repo_identifier.trim();
    if identifier.contains('/') {
        validate_owner_repo_format(identifier)?;
        let normalized = identifier
            .split('/')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("/");
        repos
            .iter()
            .find(|r| r.full_name == normalized)
            .map(|r| r.id.clone())
            .context(format!(
                "Repository '{normalized}' not found. Make sure you have access to this repository."
            ))
    } else {
        match_repo_by_name(identifier, repos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Repo;

    fn sample_repos() -> Vec<Repo> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "repo_1", "name": "cli", "ownerName": "usedetail",
                "fullName": "usedetail/cli", "visibility": "public",
                "primaryBranch": "main", "orgId": "org_1", "orgName": "Detail"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "repo_2", "name": "cli", "ownerName": "acme",
                "fullName": "acme/cli", "visibility": "private",
                "primaryBranch": "main", "orgId": "org_2", "orgName": "Acme"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "repo_3", "name": "web", "ownerName": "usedetail",
                "fullName": "usedetail/web", "visibility": "public",
                "primaryBranch": "main", "orgId": "org_1", "orgName": "Detail"
            }))
            .unwrap(),
        ]
    }

    // ── validate_owner_repo_format ───────────────────────────────────

    #[test]
    fn valid_owner_repo() {
        assert!(validate_owner_repo_format("usedetail/cli").is_ok());
    }

    #[test]
    fn rejects_empty_owner() {
        assert!(validate_owner_repo_format("/cli").is_err());
    }

    #[test]
    fn rejects_empty_repo() {
        assert!(validate_owner_repo_format("usedetail/").is_err());
    }

    #[test]
    fn rejects_multiple_slashes() {
        assert!(validate_owner_repo_format("a/b/c").is_err());
    }

    #[test]
    fn rejects_slash_only() {
        assert!(validate_owner_repo_format("/").is_err());
    }

    #[test]
    fn rejects_whitespace_only_owner() {
        assert!(validate_owner_repo_format(" /cli").is_err());
    }

    #[test]
    fn rejects_whitespace_only_repo() {
        assert!(validate_owner_repo_format("owner/ ").is_err());
    }

    #[test]
    fn accepts_unicode_owner_repo() {
        // Unicode characters are valid — GitHub doesn't allow them, but
        // validation is the API's job; we just check the format.
        assert!(validate_owner_repo_format("ünïcödé/repo").is_ok());
    }

    #[test]
    fn rejects_empty_string() {
        assert!(validate_owner_repo_format("").is_err());
    }

    // ── match_repo_by_name ───────────────────────────────────────────

    #[test]
    fn match_single_repo_by_name() {
        let repos = sample_repos();
        let id = match_repo_by_name("web", &repos).unwrap();
        assert_eq!(id.to_string(), "repo_3");
    }

    #[test]
    fn match_no_repo_returns_error() {
        let repos = sample_repos();
        let err = match_repo_by_name("nonexistent", &repos).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn match_multiple_repos_returns_error_with_names() {
        let repos = sample_repos();
        let err = match_repo_by_name("cli", &repos).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Multiple repositories"));
        assert!(msg.contains("usedetail/cli"));
        assert!(msg.contains("acme/cli"));
    }

    #[test]
    fn match_empty_repo_list() {
        let err = match_repo_by_name("cli", &[]).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // ── resolve_repo_id_from_repos ──────────────────────────────────

    #[test]
    fn resolve_owner_repo_exact_match() {
        let repos = sample_repos();
        let id = resolve_repo_id_from_repos(&repos, "usedetail/cli").unwrap();
        assert_eq!(id.to_string(), "repo_1");
    }

    #[test]
    fn resolve_owner_repo_not_found_has_access_hint() {
        let repos = sample_repos();
        let err = resolve_repo_id_from_repos(&repos, "usedetail/missing").unwrap_err();
        assert!(err
            .to_string()
            .contains("Make sure you have access to this repository"));
    }

    #[test]
    fn resolve_owner_repo_invalid_format_rejected() {
        let repos = sample_repos();
        let err = resolve_repo_id_from_repos(&repos, "usedetail/cli/extra").unwrap_err();
        assert!(err.to_string().contains("Invalid repository format"));
    }

    #[test]
    fn resolve_bare_repo_name_unique_match() {
        let repos = sample_repos();
        let id = resolve_repo_id_from_repos(&repos, "web").unwrap();
        assert_eq!(id.to_string(), "repo_3");
    }

    #[test]
    fn resolve_bare_repo_name_ambiguous_returns_error() {
        let repos = sample_repos();
        let err = resolve_repo_id_from_repos(&repos, "cli").unwrap_err();
        assert!(err.to_string().contains("Multiple repositories"));
    }

    #[test]
    fn resolve_owner_repo_trims_surrounding_whitespace() {
        let repos = sample_repos();
        let id = resolve_repo_id_from_repos(&repos, "  usedetail/cli  ").unwrap();
        assert_eq!(id.to_string(), "repo_1");
    }

    #[test]
    fn resolve_owner_repo_trims_around_slash() {
        let repos = sample_repos();
        let id = resolve_repo_id_from_repos(&repos, "usedetail / cli").unwrap();
        assert_eq!(id.to_string(), "repo_1");
    }

    #[test]
    fn resolve_bare_name_trims_whitespace() {
        let repos = sample_repos();
        let id = resolve_repo_id_from_repos(&repos, "  web  ").unwrap();
        assert_eq!(id.to_string(), "repo_3");
    }

    #[test]
    fn resolve_owner_repo_not_found_error_uses_normalized_form() {
        // The "not found" hint should quote the cleaned identifier, not the
        // raw whitespace-padded input.
        let repos = sample_repos();
        let err = resolve_repo_id_from_repos(&repos, "  usedetail/missing  ").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("'usedetail/missing'"), "got: {msg}");
    }
}
