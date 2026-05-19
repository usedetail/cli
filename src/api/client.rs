use std::fmt::Debug;
use std::num::NonZeroU64;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};

use progenitor::progenitor_client::{Error as ProgenitorError, ResponseValue};

/// Convert a progenitor client error into a concise anyhow error.
///
/// progenitor's own `Display` for `ErrorResponse` dumps headers and the typed
/// body via `Debug`, which is the verbose output the CLI is trying to avoid.
/// This collapses HTTP error responses to `<status> <reason>: <message>` so
/// the chain stays actionable (e.g. "401 Unauthorized") without leaking
/// internal struct shape.
#[allow(
    clippy::needless_pass_by_value,
    reason = "shape matches map_err's FnOnce(E) -> F"
)]
fn api_error<E: Debug + Serialize>(e: ProgenitorError<E>) -> anyhow::Error {
    if let ProgenitorError::ErrorResponse(rv) = &e {
        let status = rv.status();
        let head = format!(
            "API error: {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("HTTP error"),
        );
        let msg = serde_json::to_value(rv.as_ref())
            .ok()
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(serde_json::Value::as_str)
            .filter(|m| !m.is_empty())
            .map(str::to_owned);
        return msg.map_or_else(
            || anyhow::anyhow!("{head}"),
            |m| anyhow::anyhow!("{head}: {m}"),
        );
    }
    if let Some(status) = e.status() {
        return anyhow::anyhow!(
            "API error: {} {}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("HTTP error"),
        );
    }
    anyhow::anyhow!("API error: {e}")
}

use super::generated::types::CreateRuleBody;
use super::types::{
    Bug, BugDismissalReason, BugId, BugReview, BugReviewState, BugsResponse,
    CreatePublicBugReviewBody, CreateRuleInput, CreateRuleResponse,
    ListPublicBugsWorkflowRequestId, RepoId, ReposResponse, Rule, RuleCreationRequestId, RuleId,
    RuleRequestStatus, RuleRequestsResponse, RulesResponse, ScansResponse, UserInfo,
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
            .map_err(api_error)
    }

    pub async fn list_bugs(
        &self,
        repo_id: &RepoId,
        status: BugReviewState,
        limit: u32,
        offset: u32,
        scan_id: Option<&ListPublicBugsWorkflowRequestId>,
    ) -> Result<BugsResponse> {
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
            .map_err(api_error)
    }

    pub async fn get_bug(&self, bug_id: &BugId) -> Result<Bug> {
        self.inner
            .get_public_bug(bug_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
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
            .map_err(api_error)
    }

    pub async fn list_scans(
        &self,
        repo_id: &RepoId,
        limit: u32,
        offset: u32,
    ) -> Result<ScansResponse> {
        self.inner
            .list_public_scans(NonZeroU64::new(limit.into()), Some(offset.into()), repo_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn list_repos(&self, limit: u32, offset: u32) -> Result<ReposResponse> {
        self.inner
            .list_public_repos(NonZeroU64::new(limit.into()), Some(offset.into()))
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn create_rule(
        &self,
        repo_id: &RepoId,
        input: CreateRuleInput,
    ) -> Result<CreateRuleResponse> {
        let body = CreateRuleBody {
            repo_id: repo_id.clone(),
            input,
        };
        self.inner
            .create_rule(&body)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn list_rules(&self, repo_id: &RepoId) -> Result<RulesResponse> {
        self.inner
            .list_rules(repo_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn get_rule(&self, rule_id: &RuleId) -> Result<Rule> {
        self.inner
            .get_rule(rule_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn get_rule_request(
        &self,
        rcr_id: &RuleCreationRequestId,
    ) -> Result<RuleRequestStatus> {
        self.inner
            .get_rule_request(rcr_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
    }

    pub async fn list_rule_requests(&self, repo_id: &RepoId) -> Result<RuleRequestsResponse> {
        self.inner
            .list_rule_requests(repo_id)
            .await
            .map(ResponseValue::into_inner)
            .map_err(api_error)
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
