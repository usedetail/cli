use anyhow::{Context, Result};
use clap::Subcommand;

use crate::api::client::ApiClient;
use crate::api::types::{RepoId, Scan, ScanType, ScansResponse, WorkflowStatus};
use crate::output::output_list;
use crate::utils::datetime::parse_time_spec;
use crate::utils::git::resolve_repo_arg;
use crate::utils::pagination::page_to_offset;
use crate::utils::repos::resolve_repo_id;

#[derive(Subcommand)]
pub enum ScanCommands {
    /// List recent scans for a repository
    List {
        /// Repository in owner/repo format or just repo name.
        /// If omitted, inferred from the git remote (origin).
        repo: Option<String>,

        /// Filter by workflow status (e.g. failed scans in the last day).
        #[arg(long, value_enum)]
        status: Option<WorkflowStatus>,

        /// Filter by scan type.
        #[arg(long, value_enum)]
        scan_type: Option<ScanType>,

        /// Only show scans created at or after this point.
        /// Accepts a duration (e.g. 1d, 24h, 30m), an ISO date
        /// (YYYY-MM-DD), or an RFC3339 timestamp.
        #[arg(long)]
        since: Option<String>,

        /// Only show scans created at or before this point. Same forms as --since.
        #[arg(long)]
        until: Option<String>,

        /// Maximum number of results per page
        #[arg(long, default_value = "50", value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,

        /// Page number (starts at 1)
        #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(1..))]
        page: u32,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },
}

/// Page size used when fanning out to fetch every scan for client-side
/// filtering. Matches the bugs path.
const SCAN_PAGE_SIZE: u32 = 100;

/// Fetch every scan for a repo, paginating through the API server-side.
async fn fetch_all_scans(client: &ApiClient, repo_id: &RepoId) -> Result<Vec<Scan>> {
    let mut all = Vec::new();
    let mut offset = 0;

    loop {
        let response: ScansResponse = client
            .list_scans(repo_id, SCAN_PAGE_SIZE, offset)
            .await
            .context("Failed to fetch scans")?;

        let total = usize::try_from(response.total.max(0)).unwrap_or(0);
        let page_len = response.scans.len();
        all.extend(response.scans);

        if page_len == 0 || (usize::try_from(offset).unwrap_or(0) + page_len) >= total {
            break;
        }
        offset += u32::try_from(page_len).unwrap_or(u32::MAX);
    }

    Ok(all)
}

fn filter_scans(
    scans: &[Scan],
    status: Option<&WorkflowStatus>,
    scan_type: Option<&ScanType>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
) -> Vec<Scan> {
    scans
        .iter()
        .filter(|s| status.is_none_or(|want| s.workflow_status.as_ref() == Some(want)))
        .filter(|s| scan_type.is_none_or(|want| s.scan_type.as_ref() == Some(want)))
        .filter(|s| since_ms.is_none_or(|m| s.created_at >= m))
        .filter(|s| until_ms.is_none_or(|m| s.created_at <= m))
        .cloned()
        .collect()
}

fn paginate_items<T: Clone>(items: &[T], page: u32, limit: u32) -> Vec<T> {
    let offset = usize::try_from(page_to_offset(page, limit)).unwrap_or(0);
    items
        .iter()
        .skip(offset)
        .take(usize::try_from(limit).unwrap_or(0))
        .cloned()
        .collect()
}

pub async fn handle(command: &ScanCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        ScanCommands::List {
            repo,
            status,
            scan_type,
            since,
            until,
            limit,
            page,
            format,
        } => {
            let repo = resolve_repo_arg(repo.as_deref())?;
            let repo_id = resolve_repo_id(&client, &repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let now = chrono::Utc::now();
            let since_ms = since
                .as_deref()
                .map(|s| {
                    parse_time_spec(s, now)
                        .map(|dt| dt.timestamp_millis())
                        .map_err(|e| anyhow::anyhow!("invalid --since value: {e}"))
                })
                .transpose()?;
            let until_ms = until
                .as_deref()
                .map(|s| {
                    parse_time_spec(s, now)
                        .map(|dt| dt.timestamp_millis())
                        .map_err(|e| anyhow::anyhow!("invalid --until value: {e}"))
                })
                .transpose()?;

            // The scans API has no server-side filter today, so any active
            // filter forces the all-fetch path; otherwise hit the cheaper
            // single-page server fetch.
            let needs_full_fetch =
                status.is_some() || scan_type.is_some() || since_ms.is_some() || until_ms.is_some();

            if needs_full_fetch {
                let all = fetch_all_scans(&client, &repo_id).await?;
                let filtered = filter_scans(
                    &all,
                    status.as_ref(),
                    scan_type.as_ref(),
                    since_ms,
                    until_ms,
                );
                let total = filtered.len();
                let page_items = paginate_items(&filtered, *page, *limit);
                output_list(&page_items, total, *page, *limit, format)
            } else {
                let offset = page_to_offset(*page, *limit);
                let scans = client
                    .list_scans(&repo_id, *limit, offset)
                    .await
                    .context("Failed to fetch scans")?;

                output_list(
                    &scans.scans,
                    usize::try_from(scans.total.max(0)).unwrap_or(0),
                    *page,
                    *limit,
                    format,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scans() -> Vec<Scan> {
        vec![
            serde_json::from_value(serde_json::json!({
                "id": "wr_a", "repoId": "repo_1", "ownerName": "u", "repoName": "c",
                "initiator": "scheduler", "createdAt": 1_000,
                "completedAt": null, "commitSha": "abc",
                "workflowStatus": "complete", "scanType": "default",
                "workflowRequestId": "wr_a"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "wr_b", "repoId": "repo_1", "ownerName": "u", "repoName": "c",
                "initiator": "scheduler", "createdAt": 2_000,
                "completedAt": null, "commitSha": "def",
                "workflowStatus": "failed", "scanType": "recentChanges",
                "workflowRequestId": "wr_b"
            }))
            .unwrap(),
            serde_json::from_value(serde_json::json!({
                "id": "wr_c", "repoId": "repo_1", "ownerName": "u", "repoName": "c",
                "initiator": "scheduler", "createdAt": 3_000,
                "completedAt": null, "commitSha": "ghi",
                "workflowStatus": "complete", "scanType": "recentChanges",
                "workflowRequestId": "wr_c"
            }))
            .unwrap(),
        ]
    }

    #[test]
    fn filter_no_filters_returns_all() {
        let scans = sample_scans();
        assert_eq!(filter_scans(&scans, None, None, None, None).len(), 3);
    }

    #[test]
    fn filter_by_status_failed() {
        let scans = sample_scans();
        let f = filter_scans(&scans, Some(&WorkflowStatus::Failed), None, None, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].workflow_request_id.as_deref().unwrap_or(""), "wr_b");
    }

    #[test]
    fn filter_by_status_complete() {
        let scans = sample_scans();
        let f = filter_scans(&scans, Some(&WorkflowStatus::Complete), None, None, None);
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn filter_by_scan_type_default() {
        let scans = sample_scans();
        let f = filter_scans(&scans, None, Some(&ScanType::Default), None, None);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].workflow_request_id.as_deref().unwrap_or(""), "wr_a");
    }

    #[test]
    fn filter_by_scan_type_recent_changes() {
        let scans = sample_scans();
        let f = filter_scans(&scans, None, Some(&ScanType::RecentChanges), None, None);
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn filter_combines_status_and_scan_type_as_and() {
        // failed AND recent_changes → just scan_b
        let scans = sample_scans();
        let f = filter_scans(
            &scans,
            Some(&WorkflowStatus::Failed),
            Some(&ScanType::RecentChanges),
            None,
            None,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].workflow_request_id.as_deref().unwrap_or(""), "wr_b");
    }

    #[test]
    fn filter_since_inclusive_lower_bound() {
        let scans = sample_scans();
        let f = filter_scans(&scans, None, None, Some(2_000), None);
        let ids: Vec<_> = f
            .iter()
            .map(|s| s.workflow_request_id.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(ids, vec!["wr_b", "wr_c"]);
    }

    #[test]
    fn filter_until_inclusive_upper_bound() {
        let scans = sample_scans();
        let f = filter_scans(&scans, None, None, None, Some(2_000));
        let ids: Vec<_> = f
            .iter()
            .map(|s| s.workflow_request_id.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(ids, vec!["wr_a", "wr_b"]);
    }

    #[test]
    fn filter_window_with_status() {
        // since=2000 narrows to scan_b/c; status=complete narrows to scan_c.
        let scans = sample_scans();
        let f = filter_scans(
            &scans,
            Some(&WorkflowStatus::Complete),
            None,
            Some(2_000),
            None,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].workflow_request_id.as_deref().unwrap_or(""), "wr_c");
    }
}
