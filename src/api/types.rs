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
    fn csv_headers() -> &'static [&'static str] {
        &["id", "title", "file", "created"]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.title.clone(),
            self.file_path.as_deref().unwrap_or("-").to_string(),
            format_date(self.created_at),
        ]
    }

    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        (
            self.title.clone(),
            vec![
                ("Bug ID", self.id.to_string()),
                ("Created", format_date(self.created_at)),
            ],
        )
    }
}

impl Formattable for Repo {
    fn csv_headers() -> &'static [&'static str] {
        &["repository", "organization"]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![self.full_name.clone(), self.org_name.clone()]
    }

    fn to_card(&self) -> (String, Vec<(&'static str, String)>) {
        (
            self.full_name.clone(),
            vec![("Organization", self.org_name.clone())],
        )
    }
}
