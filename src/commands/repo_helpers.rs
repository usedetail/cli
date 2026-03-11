use anyhow::{bail, Context, Result};

use crate::api::client::ApiClient;
use crate::api::types::{Repo, RepoId};

/// Page size used when paginating through repos to resolve identifiers.
const REPO_PAGE_SIZE: u32 = 100;

/// Fetch all repos by paginating through the API.
async fn fetch_all_repos(client: &ApiClient) -> Result<Vec<Repo>> {
    let mut all_repos = Vec::new();
    let mut offset = 0;

    loop {
        let repos = client
            .list_repos(REPO_PAGE_SIZE, offset)
            .await
            .context("Failed to fetch repositories while resolving identifier")?;

        let page_size = repos.repos.len();
        all_repos.extend(repos.repos);

        if page_size < usize::try_from(REPO_PAGE_SIZE).unwrap_or(0) {
            break;
        }
        offset += REPO_PAGE_SIZE;
    }

    Ok(all_repos)
}

/// Validate that a slash-containing identifier has exactly one slash with
/// non-empty owner and repo parts.
pub(crate) fn validate_owner_repo_format(identifier: &str) -> Result<()> {
    let parts: Vec<&str> = identifier.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Invalid repository format. Please use owner/repo (e.g., 'usedetail/cli') or just the repo name. Run 'detail repos list' to see your repositories."
        );
    }
    Ok(())
}

/// Given a bare repo name and the full list of accessible repos, return the
/// matching repo ID — or a helpful error when zero or multiple repos match.
pub(crate) fn match_repo_by_name(name: &str, repos: &[Repo]) -> Result<RepoId> {
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
pub(crate) async fn resolve_repo_id(client: &ApiClient, repo_identifier: &str) -> Result<RepoId> {
    let repos = fetch_all_repos(client).await?;
    resolve_repo_id_from_repos(&repos, repo_identifier)
}

pub(crate) fn resolve_repo_id_from_repos(repos: &[Repo], repo_identifier: &str) -> Result<RepoId> {
    if repo_identifier.contains('/') {
        validate_owner_repo_format(repo_identifier)?;
        repos
            .iter()
            .find(|r| r.full_name == repo_identifier)
            .map(|r| r.id.clone())
            .context(format!(
                "Repository '{repo_identifier}' not found. Make sure you have access to this repository."
            ))
    } else {
        match_repo_by_name(repo_identifier, repos)
    }
}
