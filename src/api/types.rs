use clap::builder::PossibleValue;

use crate::output::Formattable;
use crate::utils::datetime::{format_date, format_datetime};

// Re-export generated types as the public API for this crate.
pub use super::generated::types::{
    Bug, BugCounts, BugDismissalReason, BugId, BugReview, BugReviewId, BugReviewState,
    CreatePublicBugReviewBody, CreateRuleInput, CreateRuleResponse, IntroducedIn, LinkedIssue,
    LinkedIssueTracker, ListPublicBugsWorkflowRequestId, Org, OrgId, Repo, RepoId, Rule,
    RuleCreationRequestId, RuleId, RuleListItem, RuleRequestResult, RuleRequestStatus, RuleStatus,
    Scan, ScanInitiator, ScanType, WorkflowStatus,
};

// Friendlier aliases for the generated response-wrapper names.
pub type UserInfo = super::generated::types::GetPublicUserResponse;
pub type BugsResponse = super::generated::types::ListPublicBugsResponse;
pub type ReposResponse = super::generated::types::ListPublicReposResponse;
pub type ScansResponse = super::generated::types::ListPublicScansResponse;
pub type RulesResponse = super::generated::types::ListRulesResponse;
pub type RuleRequestsResponse = super::generated::types::ListRuleRequestsResponse;

// ── Display helpers ──────────────────────────────────────────────────
// progenitor already implements Display for the generated enums, so we
// provide standalone helpers for user-friendly labels where needed.

pub const fn review_state_label(s: &BugReviewState) -> &'static str {
    match s {
        BugReviewState::Pending => "Pending",
        BugReviewState::Resolved => "Resolved",
        BugReviewState::Dismissed => "Dismissed",
    }
}

pub const fn dismissal_reason_label(r: &BugDismissalReason) -> &'static str {
    match r {
        BugDismissalReason::NotABug => "Not a Bug",
        BugDismissalReason::WontFix => "Won't Fix",
        BugDismissalReason::Duplicate => "Duplicate",
        BugDismissalReason::Other => "Other",
    }
}

pub const fn rule_status_label(s: &RuleStatus) -> &'static str {
    match s {
        RuleStatus::Pending => "Pending",
        RuleStatus::Complete => "Complete",
        RuleStatus::Failed => "Failed",
    }
}

/// Format blame/attribution info for display, e.g. "PR #42 (abc1234) on 2024-12-23 by alice".
pub fn format_introduced_in(intro: &IntroducedIn) -> String {
    let commit = intro.sha.get(..7).unwrap_or(&intro.sha);
    let ref_label = intro
        .pr_number
        .map_or_else(|| commit.to_string(), |pr| format!("PR #{pr} ({commit})"));
    let date_part = format!(" on {}", intro.date);
    let author_part = intro
        .author
        .as_deref()
        .map(|a| format!(" by {a}"))
        .unwrap_or_default();
    format!("{ref_label}{date_part}{author_part}")
}

/// Format a linked issue for display. Includes the URL for the detail/show view.
pub fn format_linked_issue(issue: &LinkedIssue) -> String {
    let tracker = issue.tracker.to_string();
    match issue.tracker {
        LinkedIssueTracker::Slack => issue
            .url
            .as_deref()
            .map_or_else(|| tracker.clone(), |url| format!("{tracker}: {url}")),
        LinkedIssueTracker::Linear | LinkedIssueTracker::Jira | LinkedIssueTracker::Github => {
            issue.url.as_ref().map_or_else(
                || format!("{tracker}: {}", issue.issue_id),
                |url| format!("{tracker}: {} \u{2014} {url}", issue.issue_id),
            )
        }
    }
}

// ── clap::ValueEnum ──────────────────────────────────────────────────

impl clap::ValueEnum for BugReviewState {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Pending, Self::Resolved, Self::Dismissed]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::Pending => Some(PossibleValue::new("pending")),
            Self::Resolved => Some(PossibleValue::new("resolved")),
            Self::Dismissed => Some(PossibleValue::new("dismissed")),
        }
    }
}

impl clap::ValueEnum for BugDismissalReason {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::NotABug, Self::WontFix, Self::Duplicate, Self::Other]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::NotABug => Some(PossibleValue::new("not-a-bug")),
            Self::WontFix => Some(PossibleValue::new("wont-fix")),
            Self::Duplicate => Some(PossibleValue::new("duplicate")),
            Self::Other => Some(PossibleValue::new("other")),
        }
    }
}

// ── Formattable ──────────────────────────────────────────────────────

impl Formattable for Bug {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        let mut pairs = vec![
            ("Bug ID", self.id.to_string()),
            ("Created", format_date(self.created_at)),
        ];
        // Each remaining field is conditional so non-applicable bugs don't
        // get a forest of "-" rows. Triage threads regularly need file path
        // and introducing PR — pulling them straight from `--format json`
        // costs no extra round-trip vs. the previous N+1 `bugs show` loop.
        if let Some(path) = self.file_path.as_deref() {
            pairs.push(("File", path.to_string()));
        }
        if self.is_security_vulnerability == Some(true) {
            pairs.push(("Security", "Yes".to_string()));
        }
        if let Some(intro) = &self.introduced_in {
            pairs.push(("Introduced", format_introduced_in(intro)));
        }
        if !self.linked_issues.is_empty() {
            let formatted = self
                .linked_issues
                .iter()
                .map(|i| match i.tracker {
                    LinkedIssueTracker::Slack => i.tracker.to_string(),
                    LinkedIssueTracker::Linear
                    | LinkedIssueTracker::Jira
                    | LinkedIssueTracker::Github => format!("{}: {}", i.tracker, i.issue_id),
                })
                .collect::<Vec<_>>()
                .join(", ");
            pairs.push(("Linked Issues", formatted));
        }
        (self.title.clone(), pairs)
    }
}

impl Formattable for Scan {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        let repo = format!("{}/{}", self.owner_name, self.repo_name);
        // Drop the "(N Open)" parenthetical when it would be noise:
        //  - total == 0: "0 Bugs Found (0 Open)" reads as boilerplate; the
        //    leading count already conveys "nothing"
        //  - open == total: "(N Open)" just restates the total
        let header = match &self.bug_counts {
            Some(counts) if counts.total > 0 && counts.open != counts.total => {
                format!("{repo} {} Bugs Found ({} Open)", counts.total, counts.open)
            }
            Some(counts) => format!("{repo} {} Bugs Found", counts.total),
            None => repo,
        };
        let mut pairs = vec![
            (
                "Status",
                self.workflow_status
                    .as_ref()
                    .map_or_else(|| "-".to_string(), ToString::to_string),
            ),
            (
                "Scan Type",
                self.scan_type
                    .as_ref()
                    .map_or_else(|| "-".to_string(), ToString::to_string),
            ),
            ("Initiator", self.initiator.to_string()),
        ];
        // Surface the commit the scan ran against so users can answer "did
        // this run on the current main?" without an extra round-trip.
        if let Some(sha) = self.commit_sha.as_deref() {
            let short = sha.get(..7).unwrap_or(sha);
            pairs.push(("Commit", short.to_string()));
        }
        pairs.push((
            "Workflow ID",
            self.workflow_request_id
                .clone()
                .unwrap_or_else(|| "-".to_string()),
        ));
        pairs.push(("Created", format_datetime(self.created_at)));
        (header, pairs)
    }
}

impl Formattable for Repo {
    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        (
            self.full_name.clone(),
            vec![("Organization", self.org_name.clone())],
        )
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;

    use super::*;

    // ── Display helpers ──────────────────────────────────────────────

    #[test]
    fn review_state_labels() {
        assert_eq!(review_state_label(&BugReviewState::Pending), "Pending");
        assert_eq!(review_state_label(&BugReviewState::Resolved), "Resolved");
        assert_eq!(review_state_label(&BugReviewState::Dismissed), "Dismissed");
    }

    #[test]
    fn dismissal_reason_labels() {
        assert_eq!(
            dismissal_reason_label(&BugDismissalReason::NotABug),
            "Not a Bug"
        );
        assert_eq!(
            dismissal_reason_label(&BugDismissalReason::WontFix),
            "Won't Fix"
        );
        assert_eq!(
            dismissal_reason_label(&BugDismissalReason::Duplicate),
            "Duplicate"
        );
        assert_eq!(dismissal_reason_label(&BugDismissalReason::Other), "Other");
    }

    // ── ValueEnum ────────────────────────────────────────────────────

    #[test]
    fn review_state_variant_count() {
        assert_eq!(BugReviewState::value_variants().len(), 3);
    }

    #[test]
    fn review_state_possible_values() {
        let values: Vec<String> = BugReviewState::value_variants()
            .iter()
            .map(|v| {
                v.to_possible_value()
                    .expect("variant has a value")
                    .get_name()
                    .to_string()
            })
            .collect();
        assert_eq!(values, vec!["pending", "resolved", "dismissed"]);
    }

    #[test]
    fn dismissal_reason_variant_count() {
        assert_eq!(BugDismissalReason::value_variants().len(), 4);
    }

    #[test]
    fn dismissal_reason_possible_values() {
        let values: Vec<String> = BugDismissalReason::value_variants()
            .iter()
            .map(|v| {
                v.to_possible_value()
                    .expect("variant has a value")
                    .get_name()
                    .to_string()
            })
            .collect();
        assert_eq!(values, vec!["not-a-bug", "wont-fix", "duplicate", "other"]);
    }

    // ── Formattable ──────────────────────────────────────────────────

    fn sample_bug() -> Bug {
        serde_json::from_value(serde_json::json!({
            "id": "bug_abc123",
            "title": "Null pointer in handler",
            "summary": "Crash when input is empty",
            "createdAt": 1_736_899_200_000_i64,
            "repoId": "repo_xyz",
            "linkedIssues": []
        }))
        .expect("valid Bug JSON")
    }

    fn sample_repo() -> Repo {
        serde_json::from_value(serde_json::json!({
            "id": "repo_xyz",
            "name": "cli",
            "ownerName": "usedetail",
            "fullName": "usedetail/cli",
            "visibility": "public",
            "primaryBranch": "main",
            "orgId": "org_001",
            "orgName": "Detail"
        }))
        .expect("valid Repo JSON")
    }

    #[test]
    fn bug_card_header_is_title() {
        let (header, _) = sample_bug().to_card();
        assert_eq!(header, "Null pointer in handler");
    }

    #[test]
    fn bug_card_contains_id_and_created() {
        let (_, pairs) = sample_bug().to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["Bug ID", "Created"]);
        assert!(pairs[0].1.contains("bug_abc123"));
        assert_eq!(pairs[1].1, format_date(1_736_899_200_000));
    }

    // The old `bug_card_omits_security_when_true` and `_hides_security_when_false`
    // tests asserted that `Security` was never emitted; that's now superseded
    // by `bug_card_includes_security_only_when_true` below, which exercises
    // all three states (None / false / true).

    #[test]
    fn bug_card_shows_linked_issues_when_present() {
        let bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_li1",
            "title": "Auth bypass",
            "summary": "...",
            "createdAt": 1_736_899_200_000_i64,
            "repoId": "repo_xyz",
            "linkedIssues": [
                { "tracker": "linear", "issueId": "ENG-42", "url": null },
                { "tracker": "jira", "issueId": "PROJ-99", "url": null }
            ]
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = bug.to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert!(keys.contains(&"Linked Issues"));
        let issues_val = &pairs.iter().find(|(k, _)| *k == "Linked Issues").unwrap().1;
        assert!(issues_val.contains("linear: ENG-42"));
        assert!(issues_val.contains("jira: PROJ-99"));
    }

    #[test]
    fn bug_card_omits_linked_issues_when_empty() {
        let (_, pairs) = sample_bug().to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert!(!keys.contains(&"Linked Issues"));
    }

    // ── richer Bug card: File / Security / Introduced ────────────────

    #[test]
    fn bug_card_omits_file_when_absent() {
        let (_, pairs) = sample_bug().to_card();
        assert!(!pairs.iter().any(|(k, _)| *k == "File"));
    }

    #[test]
    fn bug_card_includes_file_when_present() {
        let bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_filed", "title": "...", "summary": "...",
            "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
            "filePath": "src/handlers/login.rs"
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = bug.to_card();
        let value = pairs.iter().find(|(k, _)| *k == "File").map(|(_, v)| v);
        assert_eq!(value, Some(&"src/handlers/login.rs".to_string()));
    }

    #[test]
    fn bug_card_includes_security_only_when_true() {
        // None and false should *not* emit the Security row — keeps the
        // table view quiet for non-vulns.
        let none_bug = sample_bug();
        assert!(!none_bug.to_card().1.iter().any(|(k, _)| *k == "Security"));

        let false_bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_safe", "title": "...", "summary": "...",
            "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
            "isSecurityVulnerability": false
        }))
        .expect("valid Bug JSON");
        assert!(!false_bug.to_card().1.iter().any(|(k, _)| *k == "Security"));

        let true_bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_vuln", "title": "...", "summary": "...",
            "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
            "isSecurityVulnerability": true
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = true_bug.to_card();
        let v = pairs.iter().find(|(k, _)| *k == "Security").map(|(_, v)| v);
        assert_eq!(v, Some(&"Yes".to_string()));
    }

    #[test]
    fn bug_card_includes_introduced_when_present() {
        let bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_blamed", "title": "...", "summary": "...",
            "createdAt": 1, "repoId": "repo_1", "linkedIssues": [],
            "introducedIn": { "sha": "abc1234def", "date": "2024-12-23",
                              "prNumber": 42, "author": "alice" }
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = bug.to_card();
        let v = pairs
            .iter()
            .find(|(k, _)| *k == "Introduced")
            .map(|(_, v)| v);
        assert_eq!(
            v,
            Some(&"PR #42 (abc1234) on 2024-12-23 by alice".to_string())
        );
    }

    // ── format_introduced_in ─────────────────────────────────────────

    #[test]
    fn format_introduced_in_full_sha_with_pr_and_author() {
        let intro = IntroducedIn {
            sha: "abc1234def5678".to_string(),
            date: "2024-12-23".to_string(),
            pr_number: Some(42),
            author: Some("alice".to_string()),
        };
        assert_eq!(
            format_introduced_in(&intro),
            "PR #42 (abc1234) on 2024-12-23 by alice"
        );
    }

    #[test]
    fn format_introduced_in_no_pr_number() {
        let intro = IntroducedIn {
            sha: "abc1234def5678".to_string(),
            date: "2024-12-23".to_string(),
            pr_number: None,
            author: Some("bob".to_string()),
        };
        assert_eq!(format_introduced_in(&intro), "abc1234 on 2024-12-23 by bob");
    }

    #[test]
    fn format_introduced_in_no_author() {
        let intro = IntroducedIn {
            sha: "abc1234def5678".to_string(),
            date: "2024-12-23".to_string(),
            pr_number: Some(99),
            author: None,
        };
        assert_eq!(
            format_introduced_in(&intro),
            "PR #99 (abc1234) on 2024-12-23"
        );
    }

    #[test]
    fn format_introduced_in_no_pr_no_author() {
        let intro = IntroducedIn {
            sha: "abc1234def5678".to_string(),
            date: "2024-12-23".to_string(),
            pr_number: None,
            author: None,
        };
        assert_eq!(format_introduced_in(&intro), "abc1234 on 2024-12-23");
    }

    #[test]
    fn format_introduced_in_short_sha() {
        let intro = IntroducedIn {
            sha: "abc".to_string(),
            date: "2024-01-01".to_string(),
            pr_number: None,
            author: None,
        };
        assert_eq!(format_introduced_in(&intro), "abc on 2024-01-01");
    }

    #[test]
    fn format_introduced_in_exactly_7_char_sha() {
        let intro = IntroducedIn {
            sha: "abc1234".to_string(),
            date: "2024-01-01".to_string(),
            pr_number: None,
            author: None,
        };
        assert_eq!(format_introduced_in(&intro), "abc1234 on 2024-01-01");
    }

    #[test]
    fn format_introduced_in_empty_sha() {
        let intro = IntroducedIn {
            sha: String::new(),
            date: "2024-01-01".to_string(),
            pr_number: None,
            author: None,
        };
        assert_eq!(format_introduced_in(&intro), " on 2024-01-01");
    }

    #[test]
    fn format_linked_issue_linear_with_url() {
        let issue: LinkedIssue = serde_json::from_value(serde_json::json!({
            "tracker": "linear",
            "issueId": "ENG-42",
            "url": "https://linear.app/team/issue/ENG-42"
        }))
        .expect("valid LinkedIssue JSON");
        let result = format_linked_issue(&issue);
        assert_eq!(
            result,
            "linear: ENG-42 \u{2014} https://linear.app/team/issue/ENG-42"
        );
    }

    #[test]
    fn format_linked_issue_slack_with_url() {
        let issue: LinkedIssue = serde_json::from_value(serde_json::json!({
            "tracker": "slack",
            "issueId": "",
            "url": "https://workspace.slack.com/archives/C123/p456"
        }))
        .expect("valid LinkedIssue JSON");
        let result = format_linked_issue(&issue);
        assert_eq!(
            result,
            "slack: https://workspace.slack.com/archives/C123/p456"
        );
    }

    #[test]
    fn format_linked_issue_slack_no_url() {
        let issue: LinkedIssue = serde_json::from_value(serde_json::json!({
            "tracker": "slack",
            "issueId": "",
            "url": null
        }))
        .expect("valid LinkedIssue JSON");
        let result = format_linked_issue(&issue);
        assert_eq!(result, "slack");
    }

    #[test]
    fn format_linked_issue_github_no_url() {
        let issue: LinkedIssue = serde_json::from_value(serde_json::json!({
            "tracker": "github",
            "issueId": "#123",
            "url": null
        }))
        .expect("valid LinkedIssue JSON");
        let result = format_linked_issue(&issue);
        assert_eq!(result, "github: #123");
    }

    #[test]
    fn repo_card_header_is_full_name() {
        let (header, _) = sample_repo().to_card();
        assert_eq!(header, "usedetail/cli");
    }

    #[test]
    fn repo_card_contains_org() {
        let (_, pairs) = sample_repo().to_card();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("Organization", "Detail".to_string()));
    }

    // ── Scan Formattable ─────────────────────────────────────────────

    fn sample_scan() -> Scan {
        serde_json::from_value(serde_json::json!({
            "id": "scan_001",
            "repoId": "repo_001",
            "ownerName": "usedetail",
            "repoName": "cli",
            "initiator": "scheduler",
            "createdAt": 1_736_899_200_000_i64,
            "completedAt": 1_736_899_300_000_i64,
            "commitSha": "abc123",
            "workflowStatus": "complete",
            "scanType": "default",
            "workflowRequestId": "wr_abc123",
            "bugCounts": { "total": 5, "open": 3, "dismissed": 1, "resolved": 1 }
        }))
        .expect("valid Scan JSON")
    }

    fn scan_pair<'a>(pairs: &'a [(&'static str, String)], key: &str) -> Option<&'a String> {
        pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    #[test]
    fn scan_card_header_with_bug_counts() {
        let (header, _) = sample_scan().to_card();
        assert_eq!(header, "usedetail/cli 5 Bugs Found (3 Open)");
    }

    #[test]
    fn scan_card_header_drops_parenthetical_when_total_is_zero() {
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_zero", "repoId": "repo_001",
            "ownerName": "usedetail", "repoName": "cli",
            "initiator": "scheduler", "createdAt": 1,
            "completedAt": null, "commitSha": "abc123",
            "workflowRequestId": null,
            "bugCounts": { "total": 0, "open": 0, "dismissed": 0, "resolved": 0 }
        }))
        .expect("valid Scan JSON");
        let (header, _) = scan.to_card();
        assert_eq!(header, "usedetail/cli 0 Bugs Found");
    }

    #[test]
    fn scan_card_header_drops_parenthetical_when_open_equals_total() {
        // 7 found, 7 open: "(7 Open)" is just restating the total.
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_alldopen", "repoId": "repo_001",
            "ownerName": "usedetail", "repoName": "cli",
            "initiator": "scheduler", "createdAt": 1,
            "completedAt": null, "commitSha": "abc123",
            "workflowRequestId": null,
            "bugCounts": { "total": 7, "open": 7, "dismissed": 0, "resolved": 0 }
        }))
        .expect("valid Scan JSON");
        let (header, _) = scan.to_card();
        assert_eq!(header, "usedetail/cli 7 Bugs Found");
    }

    #[test]
    fn scan_card_header_keeps_parenthetical_when_partially_resolved() {
        let (header, _) = sample_scan().to_card();
        assert_eq!(header, "usedetail/cli 5 Bugs Found (3 Open)");
    }

    #[test]
    fn scan_card_header_without_bug_counts() {
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_002",
            "repoId": "repo_001",
            "ownerName": "usedetail",
            "repoName": "cli",
            "initiator": "scheduler",
            "createdAt": 1_736_899_200_000_i64,
            "completedAt": null,
            "commitSha": "abc123",
            "workflowRequestId": null
        }))
        .expect("valid Scan JSON");
        let (header, _) = scan.to_card();
        assert_eq!(header, "usedetail/cli");
    }

    #[test]
    fn scan_card_contains_expected_keys() {
        let (_, pairs) = sample_scan().to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            vec![
                "Status",
                "Scan Type",
                "Initiator",
                "Commit",
                "Workflow ID",
                "Created",
            ]
        );
    }

    #[test]
    fn scan_card_includes_short_commit_sha() {
        // Truncated to 7 chars to match how introducedIn renders blame.
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_sha", "repoId": "repo_001",
            "ownerName": "usedetail", "repoName": "cli",
            "initiator": "scheduler", "createdAt": 1,
            "completedAt": null,
            "commitSha": "deadbeef1234567890abcdef",
            "workflowRequestId": null
        }))
        .expect("valid Scan JSON");
        let (_, pairs) = scan.to_card();
        assert_eq!(scan_pair(&pairs, "Commit"), Some(&"deadbee".to_string()));
    }

    #[test]
    fn scan_card_omits_commit_when_null() {
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_nosha", "repoId": "repo_001",
            "ownerName": "usedetail", "repoName": "cli",
            "initiator": "scheduler", "createdAt": 1,
            "completedAt": null, "commitSha": null,
            "workflowRequestId": null
        }))
        .expect("valid Scan JSON");
        let (_, pairs) = scan.to_card();
        assert!(scan_pair(&pairs, "Commit").is_none());
    }

    #[test]
    fn scan_card_status_none_shows_dash() {
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_003",
            "repoId": "repo_001",
            "ownerName": "usedetail",
            "repoName": "cli",
            "initiator": "scheduler",
            "createdAt": 1_736_899_200_000_i64,
            "completedAt": null,
            "commitSha": "abc123",
            "workflowRequestId": null
        }))
        .expect("valid Scan JSON");
        let (_, pairs) = scan.to_card();
        assert_eq!(scan_pair(&pairs, "Status"), Some(&"-".to_string()));
        assert_eq!(scan_pair(&pairs, "Scan Type"), Some(&"-".to_string()));
    }

    #[test]
    fn scan_card_workflow_id_none_shows_dash() {
        let scan: Scan = serde_json::from_value(serde_json::json!({
            "id": "scan_004",
            "repoId": "repo_001",
            "ownerName": "usedetail",
            "repoName": "cli",
            "initiator": "scheduler",
            "createdAt": 1_736_899_200_000_i64,
            "completedAt": null,
            "commitSha": "abc123",
            "workflowRequestId": null
        }))
        .expect("valid Scan JSON");
        let (_, pairs) = scan.to_card();
        assert_eq!(scan_pair(&pairs, "Workflow ID"), Some(&"-".to_string()));
    }

    #[test]
    fn scan_card_workflow_id_present() {
        let (_, pairs) = sample_scan().to_card();
        assert_eq!(
            scan_pair(&pairs, "Workflow ID"),
            Some(&"wr_abc123".to_string())
        );
    }
}
