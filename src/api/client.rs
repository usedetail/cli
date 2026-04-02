use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::Deserialize;

use progenitor::progenitor_client::ResponseValue;

use super::types::{
    Bug, BugDismissalReason, BugId, BugReview, BugReviewState, BugsResponse,
    CreatePublicBugReviewBody, ListPublicBugsWorkflowRequestId, RepoId, ReposResponse,
    ScansResponse, UserInfo,
};

fn base_http_client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(format!("detail-cli/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
}

pub struct ApiClient {
    inner: super::generated::Client,
}

impl ApiClient {
    pub fn new(base_url: Option<String>, token: Option<String>) -> Result<Self> {
        let base_url = base_url.unwrap_or_else(|| "https://api.detail.dev".into());

        let mut builder = base_http_client();

        if let Some(token) = token {
            let mut headers = HeaderMap::new();
            headers.insert(
                AUTHORIZATION,
                format!("Bearer {token}")
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
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
    }

    pub async fn list_bugs(
        &self,
        repo_id: &RepoId,
        status: BugReviewState,
        limit: u32,
        offset: u32,
        scan_id: Option<&ListPublicBugsWorkflowRequestId>,
    ) -> Result<BugsResponse> {
        use std::num::NonZeroU64;

        self.inner
            .list_public_bugs(
                NonZeroU64::new(limit.into()),
                Some(offset.into()),
                repo_id,
                status,
                scan_id,
            )
            .await
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
    }

    pub async fn get_bug(&self, bug_id: &BugId) -> Result<Bug> {
        self.inner
            .get_public_bug(bug_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
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
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
    }

    pub async fn list_scans(
        &self,
        repo_id: &RepoId,
        limit: u32,
        offset: u32,
    ) -> Result<ScansResponse> {
        use std::num::NonZeroU64;

        self.inner
            .list_public_scans(NonZeroU64::new(limit.into()), Some(offset.into()), repo_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
    }

    pub async fn list_repos(&self, limit: u32, offset: u32) -> Result<ReposResponse> {
        use std::num::NonZeroU64;

        self.inner
            .list_public_repos(NonZeroU64::new(limit.into()), Some(offset.into()))
            .await
            .map(ResponseValue::into_inner)
            .map_err(|e| anyhow::anyhow!("API error: {e}"))
    }
}

/// Exchange a PKCE auth code for an API token.
/// This is a free function rather than an `ApiClient` method because it runs
/// before any token exists (the code/verifier pair is the proof of identity)
pub async fn pkce_token_exchange(api_url: &str, code: &str, code_verifier: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct TokenResponse {
        token: String,
    }

    let client = base_http_client().build()?;

    let resp = client
        .post(format!("{api_url}/api/v1/cli-auth/token"))
        .json(&serde_json::json!({ "code": code, "code_verifier": code_verifier }))
        .send()
        .await
        .context("Failed to reach authentication server")?
        .error_for_status()
        .context("Token exchange rejected, code may be expired or invalid")?
        .json::<TokenResponse>()
        .await
        .context("Unexpected response from authentication server")?;

    Ok(resp.token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_base_url_succeeds() {
        let client = ApiClient::new(None, None);
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_custom_base_url_succeeds() {
        let client = ApiClient::new(Some("https://custom.api.dev".into()), None);
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_token_succeeds() {
        let client = ApiClient::new(None, Some("dtl_live_test_token".into()));
        assert!(client.is_ok());
    }

    #[test]
    fn new_with_base_url_and_token_succeeds() {
        let client = ApiClient::new(
            Some("https://custom.api.dev".into()),
            Some("dtl_live_test_token".into()),
        );
        assert!(client.is_ok());
    }
}
