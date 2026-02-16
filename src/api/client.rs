use anyhow::{Context, Result};

use super::types::*;

pub struct ApiClient {
    inner: super::generated::Client,
}

/// Convert a generated response type to one of our hand-written domain types
/// by round-tripping through JSON. The generated types handle API deserialization
/// (including timestamp quirks), then we re-deserialize into our types which
/// have ID newtypes, field renames, etc.
fn convert<T: serde::de::DeserializeOwned>(val: impl serde::Serialize) -> Result<T> {
    serde_json::from_value(serde_json::to_value(val)?).map_err(Into::into)
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
        let resp = self
            .inner
            .get_public_user()
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))?;

        convert(resp)
    }

    pub async fn list_bugs(
        &self,
        repo_id: &RepoId,
        status: Option<&BugCloseState>,
        limit: u32,
        offset: u32,
    ) -> Result<BugsResponse> {
        use std::num::NonZeroU64;

        // Convert our BugCloseState to the generated BugReviewState via JSON
        let gen_status: super::generated::types::BugReviewState = status
            .map(|s| convert(s))
            .transpose()?
            .unwrap_or(super::generated::types::BugReviewState::Pending);

        let resp = self
            .inner
            .list_public_bugs(
                NonZeroU64::new(limit as u64),
                Some(offset as u64),
                repo_id.as_str(),
                gen_status,
            )
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))?;

        convert(resp)
    }

    pub async fn get_bug(&self, bug_id: &BugId) -> Result<Bug> {
        let resp = self
            .inner
            .get_public_bug(bug_id.as_str())
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))?;

        convert(resp)
    }

    pub async fn update_bug_close(
        &self,
        bug_id: &BugId,
        state: BugCloseState,
        dismissal_reason: Option<BugDismissalReason>,
        notes: Option<&str>,
    ) -> Result<BugClose> {
        // Build the request body by converting our types through JSON
        let body: super::generated::types::CreatePublicBugReviewBody =
            serde_json::from_value(serde_json::json!({
                "state": state,
                "dismissalReason": dismissal_reason,
                "notes": notes,
            }))?;

        let resp = self
            .inner
            .create_public_bug_review(bug_id.as_str(), &body)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))?;

        convert(resp)
    }

    pub async fn list_repos(&self, limit: u32, offset: u32) -> Result<ReposResponse> {
        use std::num::NonZeroU64;

        let resp = self
            .inner
            .list_public_repos(NonZeroU64::new(limit as u64), Some(offset as u64))
            .await
            .map(|r| r.into_inner())
            .map_err(|e| anyhow::anyhow!("API error: {}", e))?;

        convert(resp)
    }
}
