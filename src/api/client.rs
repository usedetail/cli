use anyhow::{bail, Context, Result};
use colored::*;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use super::types::*;

/// Structured error response from the Detail API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
    status_code: u16,
}

pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: Option<String>, token: Option<String>) -> Result<Self> {
        let base_url = base_url.unwrap_or_else(|| "https://api.detail.dev".into());

        let client = reqwest::Client::builder()
            .user_agent(format!("detail-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url,
            token,
        })
    }

    /// Make API request with version compatibility check
    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let token = self
            .token
            .as_ref()
            .context("Not authenticated. Run `detail auth login`")?;

        let url = format!("{}{}", self.base_url, path);

        let mut request = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", token));

        if let Some(b) = body {
            request = request.json(&b);
        }

        let response = request.send().await?;

        // Check for deprecation warnings
        if let Some(deprecation) = response.headers().get("deprecation") {
            if deprecation == "true" {
                if let Some(sunset) = response.headers().get("sunset") {
                    eprintln!(
                        "{}",
                        format!(
                            "Warning: This API version will be deprecated on {}. \
                            Please update your CLI.",
                            sunset.to_str().unwrap_or("unknown date")
                        )
                        .yellow()
                    );
                }
            }
        }

        // Check API version compatibility
        if let Some(api_version) = response.headers().get("x-api-version") {
            let api_version = api_version.to_str()?;
            check_version_compatibility(api_version)?;
        }

        if !response.status().is_success() {
            let status = response.status();

            // Get response body as bytes so we can try multiple parsers
            let body_bytes = response.bytes().await?;

            // Try to parse as structured API error
            let error_message = match serde_json::from_slice::<ApiError>(&body_bytes) {
                Ok(api_err) => {
                    format!(
                        "{} ({}): {}",
                        api_err.error_type, api_err.status_code, api_err.message
                    )
                }
                Err(_) => {
                    // Fallback: treat as plain text
                    let text = String::from_utf8_lossy(&body_bytes);

                    // Check if response looks like HTML
                    if text.trim_start().starts_with('<') {
                        // It's HTML, don't dump it all
                        format!(
                            "HTTP {} - Received HTML error page (expected JSON). \
                            This might indicate the endpoint doesn't exist or there's a routing issue.",
                            status.as_u16()
                        )
                    } else if text.is_empty() {
                        format!("HTTP {} error", status.as_u16())
                    } else {
                        // Plain text error
                        format!("HTTP {} error: {}", status.as_u16(), text)
                    }
                }
            };

            bail!("API error: {}", error_message);
        }

        let data = response.json().await?;
        Ok(data)
    }

    pub async fn get_current_user(&self) -> Result<UserInfo> {
        self.request(reqwest::Method::GET, "/public/v1/user", None)
            .await
    }

    pub async fn list_bugs(
        &self,
        repo_id: &RepoId,
        status: Option<&BugReviewState>,
        limit: u32,
        offset: u32,
    ) -> Result<BugsResponse> {
        #[derive(Serialize)]
        struct ListBugsQuery<'a> {
            repo_id: &'a str,
            limit: u32,
            offset: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            status: Option<&'a BugReviewState>,
        }

        let query = ListBugsQuery {
            repo_id: repo_id.as_str(),
            limit,
            offset,
            status,
        };
        let query_string = serde_urlencoded::to_string(&query)?;
        let path = format!("/public/v1/bugs?{}", query_string);
        self.request(reqwest::Method::GET, &path, None).await
    }

    pub async fn get_bug(&self, bug_id: &BugId) -> Result<Bug> {
        let path = format!("/public/v1/bugs/{}", bug_id);
        self.request(reqwest::Method::GET, &path, None).await
    }

    pub async fn update_bug_review(
        &self,
        bug_id: &BugId,
        state: BugReviewState,
        dismissal_reason: Option<BugDismissalReason>,
        notes: Option<&str>,
    ) -> Result<BugReview> {
        let path = format!("/public/v1/bugs/{}/review", bug_id);
        let request = BugReviewRequest {
            state,
            dismissal_reason,
            notes: notes.map(String::from),
        };
        let body = serde_json::to_value(request)?;
        self.request(reqwest::Method::POST, &path, Some(body)).await
    }

    pub async fn list_repos(&self, limit: u32, offset: u32) -> Result<ReposResponse> {
        #[derive(Serialize)]
        struct ListReposQuery {
            limit: u32,
            offset: u32,
        }

        let query = ListReposQuery { limit, offset };
        let query_string = serde_urlencoded::to_string(&query)?;
        let path = format!("/public/v1/repos?{}", query_string);
        self.request(reqwest::Method::GET, &path, None).await
    }
}

fn check_version_compatibility(api_version: &str) -> Result<()> {
    // CLI v0.1.x supports API v1.x
    const SUPPORTED_API_VERSIONS: &str = "^1.0";

    let api_version = Version::parse(api_version).context("Failed to parse API version")?;

    let requirement = VersionReq::parse(SUPPORTED_API_VERSIONS)?;

    if !requirement.matches(&api_version) {
        eprintln!(
            "{}",
            format!(
                "Warning: API version {} may not be fully compatible. \
                Consider updating your CLI.",
                api_version
            )
            .yellow()
        );
    }

    Ok(())
}
