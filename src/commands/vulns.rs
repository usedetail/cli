use std::convert::TryInto;

use anyhow::{Context, Result};
use clap::Subcommand;
use console::Term;

use crate::api::client::{ApiClient, VulnListQuery};
use crate::api::types::{
    format_fix_pr, format_vuln_linked_issue, severity_label, ListPublicVulnsWorkflowRequestId,
    RepoId, Vuln, VulnId, VulnScan, VulnSeverity, VulnSortOrder, WorkflowStatus,
};
use crate::output::{clamp_page, output_list, SectionRenderer};
use crate::utils::datetime::{format_datetime, parse_time_spec};
use crate::utils::pagination::page_to_offset;
use crate::utils::repos::resolve_repo_id;
use crate::utils::vcs::resolve_repo_arg;

#[derive(Subcommand)]
pub enum VulnCommands {
    /// List security scan findings for a given repository
    List {
        /// Repository by owner/repo (e.g., usedetail/cli) or repo (e.g., cli).
        /// If omitted, inferred from the git or jj remote (origin).
        repo: Option<String>,

        /// Only show findings at these severities — repeat the flag or
        /// comma-separate values (e.g. `--severity critical,high`).
        /// Default: all severities.
        #[arg(long, value_enum, value_delimiter = ',')]
        severity: Vec<VulnSeverity>,

        /// Result ordering. `severity` puts the most severe findings first.
        #[arg(long, value_enum, default_value = "newest")]
        sort: VulnSortOrder,

        /// Filter findings to a specific security scan by workflow request ID
        #[arg(long)]
        scan_id: Option<String>,

        /// Only show findings created at or after this point.
        /// Accepts a duration (e.g. 1d, 24h, 30m) interpreted as "now minus
        /// this", an ISO date (YYYY-MM-DD), or an RFC3339 timestamp.
        #[arg(long)]
        since: Option<String>,

        /// Only show findings created at or before this point. Same forms as --since.
        #[arg(long)]
        until: Option<String>,

        /// Auto-paginate: fetch every matching finding instead of a single page.
        #[arg(long, conflicts_with_all = ["page", "limit"])]
        all: bool,

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

    /// Show the report for a security scan finding
    Show {
        /// Vuln ID
        vuln_id: String,

        /// Output format
        #[arg(long, value_enum, default_value = "table")]
        format: crate::OutputFormat,
    },

    /// List recent security scans for a repository
    Scans {
        /// Repository in owner/repo format or just repo name.
        /// If omitted, inferred from the git or jj remote (origin).
        repo: Option<String>,

        /// Filter by workflow status (e.g. failed scans in the last day).
        #[arg(long, value_enum)]
        status: Option<WorkflowStatus>,

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

/// Join `--severity` values into the comma-separated form the API takes.
///
/// Preserves first-seen order and drops repeats. Returns `None` for an empty
/// selection so the query param is omitted rather than sent empty.
fn severity_query(severities: &[VulnSeverity]) -> Option<String> {
    if severities.is_empty() {
        return None;
    }
    let mut deduped: Vec<VulnSeverity> = Vec::with_capacity(severities.len());
    for severity in severities {
        if !deduped.contains(severity) {
            deduped.push(*severity);
        }
    }
    Some(
        deduped
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
    )
}

/// Resolve a `--since` / `--until` flag value to epoch millis, flattening
/// `parse_time_spec`'s error into the top-level message.
fn resolve_time_flag(
    name: &str,
    value: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<i64>> {
    value.map_or(Ok(None), |s| {
        parse_time_spec(s, now)
            .map(|dt| Some(dt.timestamp_millis()))
            .map_err(|e| anyhow::anyhow!("invalid {name} value: {e}"))
    })
}

/// Return only findings whose `createdAt` falls within the given inclusive bounds.
fn filter_by_time_range(vulns: &[Vuln], since_ms: Option<i64>, until_ms: Option<i64>) -> Vec<Vuln> {
    vulns
        .iter()
        .filter(|v| {
            since_ms.is_none_or(|s| v.created_at >= s) && until_ms.is_none_or(|u| v.created_at <= u)
        })
        .cloned()
        .collect()
}

fn filter_scans(
    scans: &[VulnScan],
    status: Option<&WorkflowStatus>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
) -> Vec<VulnScan> {
    scans
        .iter()
        .filter(|s| status.is_none_or(|want| s.workflow_status.as_ref() == Some(want)))
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

/// Key-value rows for the human-readable `vulns show` view. The markdown
/// description is rendered beneath them.
fn vuln_show_pairs(vuln: &Vuln) -> Vec<(&'static str, String)> {
    let mut pairs: Vec<(&'static str, String)> = vec![
        ("ID", vuln.id.to_string()),
        ("Title", vuln.title.clone()),
        ("Severity", severity_label(&vuln.severity).to_string()),
        (
            "File",
            if vuln.file_path.is_empty() {
                "-".to_string()
            } else {
                vuln.file_path.clone()
            },
        ),
        ("Created", format_datetime(vuln.created_at)),
    ];
    if let Some(sha) = vuln.commit_sha.as_deref() {
        let short = sha.get(..7).unwrap_or(sha);
        pairs.push(("Commit", short.to_string()));
    }
    if let Some(wr) = &vuln.workflow_request_id {
        pairs.push(("Scan", wr.to_string()));
    }
    for issue in &vuln.linked_issues {
        pairs.push(("Issue", format_vuln_linked_issue(issue)));
    }
    if let Some(fix_pr) = &vuln.fix_pr {
        pairs.push(("Fix PR", format_fix_pr(fix_pr)));
    }
    pairs
}

/// Page size used when fetching every finding or scan for client-side filtering.
const PAGE_SIZE: u32 = 100;

/// Fetch every finding for a repo by paginating through all pages.
async fn fetch_all_vulns(
    client: &ApiClient,
    repo_id: &RepoId,
    query: VulnListQuery<'_>,
) -> Result<Vec<Vuln>> {
    let mut all = Vec::new();
    let mut offset = 0;

    loop {
        let response = client
            .list_vulns(repo_id, PAGE_SIZE, offset, query)
            .await
            .context("Failed to fetch vulns from repository")?;

        let total = usize::try_from(response.total.max(0)).unwrap_or(0);
        let page_len = response.vulns.len();
        all.extend(response.vulns);

        if page_len == 0 || (usize::try_from(offset).unwrap_or(0) + page_len) >= total {
            break;
        }
        offset += u32::try_from(page_len).unwrap_or(u32::MAX);
    }

    Ok(all)
}

/// Fetch every security scan for a repo by paginating through all pages.
async fn fetch_all_scans(client: &ApiClient, repo_id: &RepoId) -> Result<Vec<VulnScan>> {
    let mut all = Vec::new();
    let mut offset = 0;

    loop {
        let response = client
            .list_vuln_scans(repo_id, PAGE_SIZE, offset)
            .await
            .context("Failed to fetch security scans")?;

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

pub async fn handle(command: &VulnCommands, cli: &crate::Cli) -> Result<()> {
    let client = cli.create_client()?;

    match command {
        VulnCommands::List {
            repo,
            severity,
            sort,
            scan_id,
            since,
            until,
            all,
            limit,
            page,
            format,
        } => {
            let repo = resolve_repo_arg(repo.as_deref())?;
            let resolved_repo_id = resolve_repo_id(&client, &repo)
                .await
                .context("Failed to resolve repository identifier")?;

            let scan_id: Option<ListPublicVulnsWorkflowRequestId> = scan_id
                .as_deref()
                .map(TryInto::try_into)
                .transpose()
                .context("Invalid scan ID format (expected wr_...)")?;

            let now = chrono::Utc::now();
            let since_ms = resolve_time_flag("--since", since.as_deref(), now)?;
            let until_ms = resolve_time_flag("--until", until.as_deref(), now)?;

            // `--severity`, `--sort` and `--scan-id` are applied by the API;
            // only `--all` and the time window force a full fetch.
            let severity_filter = severity_query(severity);
            let query = VulnListQuery {
                severity: severity_filter.as_deref(),
                sort: Some(*sort),
                scan_id: scan_id.as_ref(),
            };

            let needs_full_fetch = *all || since_ms.is_some() || until_ms.is_some();

            if needs_full_fetch {
                let mut filtered = fetch_all_vulns(&client, &resolved_repo_id, query).await?;
                if since_ms.is_some() || until_ms.is_some() {
                    filtered = filter_by_time_range(&filtered, since_ms, until_ms);
                }
                if filtered.is_empty() && matches!(format, crate::OutputFormat::Table) {
                    Term::stdout().write_line(
                        "No security vulnerabilities found with the current filters.",
                    )?;
                }
                let total = filtered.len();
                if *all {
                    let effective_limit = u32::try_from(total.max(1)).unwrap_or(u32::MAX);
                    return output_list(&filtered, total, 1, effective_limit, format);
                }
                let page = clamp_page(*page, total, *limit);
                let page_items = paginate_items(&filtered, page, *limit);
                output_list(&page_items, total, page, *limit, format)
            } else {
                let offset = page_to_offset(*page, *limit);
                let vulns = client
                    .list_vulns(&resolved_repo_id, *limit, offset, query)
                    .await
                    .context("Failed to fetch vulns from repository")?;

                output_list(
                    &vulns.vulns,
                    usize::try_from(vulns.total.max(0)).unwrap_or(0),
                    *page,
                    *limit,
                    format,
                )
            }
        }

        VulnCommands::Show { vuln_id, format } => {
            let vuln_id: VulnId = vuln_id
                .as_str()
                .try_into()
                .context("Invalid vuln ID format (expected ssf_...)")?;
            let vuln = client
                .get_vuln(&vuln_id)
                .await
                .context("Failed to fetch vuln details")?;

            if matches!(format, crate::OutputFormat::Json) {
                Term::stdout().write_line(&serde_json::to_string_pretty(&vuln)?)?;
                return Ok(());
            }
            SectionRenderer::new()
                .key_value("", &vuln_show_pairs(&vuln))
                .markdown("", &vuln.description)
                .print()
        }

        VulnCommands::Scans {
            repo,
            status,
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
            let since_ms = resolve_time_flag("--since", since.as_deref(), now)?;
            let until_ms = resolve_time_flag("--until", until.as_deref(), now)?;

            // The security scans API has no server-side filter, so any
            // active filter forces the all-fetch path.
            let needs_full_fetch = status.is_some() || since_ms.is_some() || until_ms.is_some();

            if needs_full_fetch {
                let all = fetch_all_scans(&client, &repo_id).await?;
                let filtered = filter_scans(&all, status.as_ref(), since_ms, until_ms);
                let total = filtered.len();
                let page = clamp_page(*page, total, *limit);
                let page_items = paginate_items(&filtered, page, *limit);
                output_list(&page_items, total, page, *limit, format)
            } else {
                let offset = page_to_offset(*page, *limit);
                let scans = client
                    .list_vuln_scans(&repo_id, *limit, offset)
                    .await
                    .context("Failed to fetch security scans")?;

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

    fn make_vuln(id: &str, created_at: i64) -> Vuln {
        serde_json::from_value(serde_json::json!({
            "id": format!("ssf_{id}"),
            "title": id,
            "description": "## Flaw\n...",
            "severity": "high",
            "filePath": "src/a.rs",
            "createdAt": created_at,
            "repoId": "repo_1",
            "commitSha": null,
            "workflowRequestId": null,
            "linkedIssues": []
        }))
        .unwrap()
    }

    fn make_scan(wr: &str, created_at: i64, status: &str) -> VulnScan {
        serde_json::from_value(serde_json::json!({
            "repoId": "repo_1", "repoName": "c", "ownerName": "u",
            "workflowStatus": status, "createdAt": created_at,
            "completedAt": null, "initiator": "scheduler",
            "workflowRequestId": wr, "commitSha": null,
            "vulnCounts": { "critical": 0, "high": 1, "medium": 0, "low": 0, "total": 1 }
        }))
        .unwrap()
    }

    fn ids(vulns: &[Vuln]) -> Vec<String> {
        vulns.iter().map(|v| v.id.to_string()).collect()
    }

    // ── severity_query ───────────────────────────────────────────────

    #[test]
    fn severity_query_omits_empty_selection() {
        assert!(severity_query(&[]).is_none());
    }

    #[test]
    fn severity_query_joins_lowercase() {
        assert_eq!(
            severity_query(&[VulnSeverity::Critical, VulnSeverity::Low]).as_deref(),
            Some("critical,low")
        );
    }

    #[test]
    fn severity_query_dedupes_preserving_order() {
        assert_eq!(
            severity_query(&[
                VulnSeverity::High,
                VulnSeverity::Critical,
                VulnSeverity::High
            ])
            .as_deref(),
            Some("high,critical")
        );
    }

    // ── filter_by_time_range ─────────────────────────────────────────

    #[test]
    fn time_range_is_inclusive() {
        let vulns = vec![
            make_vuln("old", 1_000),
            make_vuln("mid", 2_000),
            make_vuln("new", 3_000),
        ];
        assert_eq!(filter_by_time_range(&vulns, None, None).len(), 3);
        assert_eq!(
            ids(&filter_by_time_range(&vulns, Some(2_000), None)),
            ["ssf_mid", "ssf_new"]
        );
        assert_eq!(
            ids(&filter_by_time_range(&vulns, None, Some(2_000))),
            ["ssf_old", "ssf_mid"]
        );
        assert!(filter_by_time_range(&vulns, Some(3_000), Some(1_000)).is_empty());
    }

    // ── filter_scans ─────────────────────────────────────────────────

    #[test]
    fn filter_scans_by_status_and_window() {
        let scans = vec![
            make_scan("wr_a", 1_000, "complete"),
            make_scan("wr_b", 2_000, "failed"),
            make_scan("wr_c", 3_000, "complete"),
        ];
        assert_eq!(filter_scans(&scans, None, None, None).len(), 3);
        let failed = filter_scans(&scans, Some(&WorkflowStatus::Failed), None, None);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].workflow_request_id.as_str(), "wr_b");
        let recent_complete =
            filter_scans(&scans, Some(&WorkflowStatus::Complete), Some(2_000), None);
        assert_eq!(recent_complete.len(), 1);
        assert_eq!(recent_complete[0].workflow_request_id.as_str(), "wr_c");
    }

    // ── show view ────────────────────────────────────────────────────

    #[test]
    fn show_pairs_minimal() {
        let pairs = vuln_show_pairs(&make_vuln("x", 1));
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, ["ID", "Title", "Severity", "File", "Created"]);
        assert_eq!(pairs[2].1, "High");
    }

    #[test]
    fn show_pairs_full() {
        let vuln: Vuln = serde_json::from_value(serde_json::json!({
            "id": "ssf_full", "title": "SSRF in fetcher", "description": "...",
            "severity": "critical", "filePath": "src/fetch.rs", "createdAt": 1,
            "repoId": "repo_1", "commitSha": "abcdef1234567",
            "workflowRequestId": "wr_123",
            "linkedIssues": [{ "tracker": "linear", "issueId": "SEC-1", "url": "https://l/SEC-1" }],
            "fixPr": { "prNumber": 7, "url": "https://gh/pr/7", "state": "open" }
        }))
        .unwrap();
        let pairs = vuln_show_pairs(&vuln);
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("Severity"), Some("Critical"));
        assert_eq!(get("Commit"), Some("abcdef1"));
        assert_eq!(get("Scan"), Some("wr_123"));
        assert_eq!(get("Issue"), Some("linear: SEC-1 \u{2014} https://l/SEC-1"));
        assert_eq!(get("Fix PR"), Some("#7 (open) \u{2014} https://gh/pr/7"));
    }

    #[test]
    fn show_pairs_dash_for_empty_file() {
        let mut vuln = make_vuln("x", 1);
        vuln.file_path = String::new();
        let pairs = vuln_show_pairs(&vuln);
        assert_eq!(pairs.iter().find(|(k, _)| *k == "File").unwrap().1, "-");
    }

    // ── paginate_items ───────────────────────────────────────────────

    #[test]
    fn paginate_items_pages() {
        let items = vec![1, 2, 3, 4, 5];
        assert_eq!(paginate_items(&items, 1, 2), vec![1, 2]);
        assert_eq!(paginate_items(&items, 3, 2), vec![5]);
        assert!(paginate_items(&items, 4, 2).is_empty());
    }
}
