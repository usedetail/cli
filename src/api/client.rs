use anyhow::{Context, Result};

use super::types::*;

pub struct ApiClient {
    inner: super::generated::Client,
}

impl ApiClient {
    pub fn new(base_url: Option<String>, token: Option<String>) -> Result<Self> {
        let base_url = base_url.unwrap_or_else(|| "https://api.detail.dev".into());

        let mut builder = reqwest::Client::builder()
            .user_agent(format!("detail-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30));

        if let Some(token) = token {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token)
                    .parse()
                    .context("Invalid token format")?,
            );
            builder = builder.default_headers(headers);
        }

        let reqwest_client = builder.build()?;
        let inner = super::generated::Client::new_with_client(&base_url, reqwest_client);

        Ok(Self { inner })
    }

    pub async fn get_current_user(&self) -> Result<UserInfo> {
        self.inner
            .get_public_user()
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))
    }

    pub async fn list_bugs(
        &self,
        repo_id: &RepoId,
        status: BugReviewState,
        limit: u32,
        offset: u32,
    ) -> Result<BugsResponse> {
        use std::num::NonZeroU64;

        self.inner
            .list_public_bugs(
                NonZeroU64::new(limit as u64),
                Some(offset as u64),
                repo_id,
                status,
            )
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))
    }

    pub async fn get_bug(&self, bug_id: &BugId) -> Result<Bug> {
        self.inner
            .get_public_bug(bug_id)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))
    }

    pub async fn update_bug_close(
        &self,
        bug_id: &BugId,
        state: BugReviewState,
        dismissal_reason: Option<BugDismissalReason>,
        notes: Option<&str>,
    ) -> Result<BugReview> {
        let body = CreatePublicBugReviewBody {
            state,
            dismissal_reason,
            notes: notes.map(String::from),
        };

        self.inner
            .create_public_bug_review(bug_id, &body)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))
    }

    pub async fn list_repos(&self, limit: u32, offset: u32) -> Result<ReposResponse> {
        use std::num::NonZeroU64;

        self.inner
            .list_public_repos(NonZeroU64::new(limit as u64), Some(offset as u64))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))
    }
}
