progenitor::generate_api!(
    spec = "openapi.json",
    replace = {
        BugReviewState = crate::api::review_state::ReviewState,
        BugReviewStateInput = crate::api::review_state::ReviewState,
        VulnReviewState = crate::api::review_state::ReviewState,
        VulnReviewStateInput = crate::api::review_state::ReviewState,
    }
);
