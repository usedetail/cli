use clap::builder::PossibleValue;

use crate::output::Formattable;
use crate::utils::format_date;

// Re-export generated types as the public API for this crate.
pub use super::generated::types::{
    Bug, BugDismissalReason, BugId, BugReview, BugReviewId, BugReviewState,
    CreatePublicBugReviewBody, Org, OrgId, Repo, RepoId,
};

// Friendlier aliases for the generated response-wrapper names.
pub type UserInfo = super::generated::types::GetPublicUserResponse;
pub type BugsResponse = super::generated::types::ListPublicBugsResponse;
pub type ReposResponse = super::generated::types::ListPublicReposResponse;

// ── Display helpers ──────────────────────────────────────────────────
// progenitor already implements Display for the generated enums, so we
// provide standalone helpers for user-friendly labels where needed.

pub fn review_state_label(s: &BugReviewState) -> &'static str {
    match s {
        BugReviewState::Pending => "Pending",
        BugReviewState::Resolved => "Resolved",
        BugReviewState::Dismissed => "Dismissed",
    }
}

pub fn dismissal_reason_label(r: &BugDismissalReason) -> &'static str {
    match r {
        BugDismissalReason::NotABug => "Not a Bug",
        BugDismissalReason::WontFix => "Won't Fix",
        BugDismissalReason::Duplicate => "Duplicate",
        BugDismissalReason::Other => "Other",
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
        let pairs = vec![
            ("Bug ID", self.id.to_string()),
            ("Created", format_date(self.created_at)),
        ];
        (self.title.clone(), pairs)
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
            "repoId": "repo_xyz"
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
        assert_eq!(pairs[1].1, "2025-01-15");
    }

    #[test]
    fn bug_card_shows_security_when_true() {
        let bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_sec1",
            "title": "XSS vulnerability",
            "summary": "...",
            "createdAt": 1_736_899_200_000_i64,
            "repoId": "repo_xyz",
            "isSecurityVulnerability": true
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = bug.to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["Bug ID", "Created"]);
    }

    #[test]
    fn bug_card_hides_security_when_false() {
        let bug: Bug = serde_json::from_value(serde_json::json!({
            "id": "bug_nosec",
            "title": "Typo in docs",
            "summary": "...",
            "createdAt": 1_736_899_200_000_i64,
            "repoId": "repo_xyz",
            "isSecurityVulnerability": false
        }))
        .expect("valid Bug JSON");
        let (_, pairs) = bug.to_card();
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["Bug ID", "Created"]);
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
}
