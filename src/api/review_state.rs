//! Review state shared by bugs and vulns.
//!
//! Hand-written instead of generated (see the `replace` list in
//! `generated.rs`) so that decoding tolerates both spellings of the open
//! state and values this CLI doesn't know yet, rather than failing the whole
//! response.

use std::fmt;

use clap::builder::PossibleValue;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Value sent to the API for [`ReviewState::Open`].
const OPEN_WIRE: &str = "open";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReviewState {
    Open,
    Resolved,
    Dismissed,
    /// A value this CLI doesn't recognise, kept verbatim so JSON output
    /// passes it through unchanged.
    Unknown(String),
}

impl ReviewState {
    /// The value the API expects for this state.
    pub fn as_wire(&self) -> &str {
        match self {
            Self::Open => OPEN_WIRE,
            Self::Resolved => "resolved",
            Self::Dismissed => "dismissed",
            Self::Unknown(raw) => raw,
        }
    }

    fn from_wire(value: &str) -> Self {
        match value {
            // Legacy alias: "pending" is accepted from servers predating the
            // open rename. Remove once unused.
            "open" | "pending" => Self::Open,
            "resolved" => Self::Resolved,
            "dismissed" => Self::Dismissed,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl fmt::Display for ReviewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

impl Serialize for ReviewState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for ReviewState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_wire(&value))
    }
}

impl clap::ValueEnum for ReviewState {
    fn value_variants<'a>() -> &'a [Self] {
        const VARIANTS: &[ReviewState] = &[
            ReviewState::Open,
            ReviewState::Resolved,
            ReviewState::Dismissed,
        ];
        VARIANTS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            // Legacy alias: accepts "pending" from users and scripts predating
            // the open rename. Remove once unused.
            Self::Open => Some(PossibleValue::new("open").alias("pending")),
            Self::Resolved => Some(PossibleValue::new("resolved")),
            Self::Dismissed => Some(PossibleValue::new("dismissed")),
            Self::Unknown(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    fn decode(value: &str) -> ReviewState {
        serde_json::from_value(serde_json::json!(value)).expect("decodes")
    }

    #[test]
    fn decodes_both_open_spellings() {
        assert_eq!(decode("open"), ReviewState::Open);
        assert_eq!(decode("pending"), ReviewState::Open);
        assert_eq!(decode("resolved"), ReviewState::Resolved);
        assert_eq!(decode("dismissed"), ReviewState::Dismissed);
    }

    #[test]
    fn unknown_value_round_trips_verbatim() {
        let state = decode("bogus");
        assert_eq!(state, ReviewState::Unknown("bogus".to_string()));
        assert_eq!(
            serde_json::to_value(&state).expect("serializes"),
            serde_json::json!("bogus")
        );
    }

    #[test]
    fn open_is_sent_as_open() {
        assert_eq!(
            serde_json::to_value(ReviewState::Open).expect("serializes"),
            serde_json::json!("open")
        );
        assert_eq!(ReviewState::Open.to_string(), "open");
    }

    #[test]
    fn legacy_pending_payload_is_printed_as_open() {
        let json = serde_json::to_value(decode("pending")).expect("serializes");
        assert_eq!(json, serde_json::json!("open"));
    }

    #[test]
    fn cli_accepts_open_and_legacy_pending() {
        for input in ["open", "pending", "OPEN"] {
            assert_eq!(
                ReviewState::from_str(input, true).expect("parses"),
                ReviewState::Open,
                "{input}"
            );
        }
        assert!(ReviewState::from_str("bogus", true).is_err());
    }

    #[test]
    fn cli_lists_open_not_pending() {
        let values: Vec<String> = ReviewState::value_variants()
            .iter()
            .filter_map(ReviewState::to_possible_value)
            .map(|v| v.get_name().to_string())
            .collect();
        assert_eq!(values, ["open", "resolved", "dismissed"]);
    }

    /// `replace` skips progenitor's generated enums, so nothing checks the
    /// spec's values at compile time. Every value the spec lists for a
    /// review-state schema must decode to a known state.
    #[test]
    fn every_spec_value_is_known() {
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../../openapi.json")).expect("valid spec");
        let schemas = &spec["components"]["schemas"];
        let names = [
            "BugReviewState",
            "BugReviewStateInput",
            "VulnReviewState",
            "VulnReviewStateInput",
        ];
        let mut seen = 0;
        for name in names {
            let Some(values) = schemas[name]["enum"].as_array() else {
                continue;
            };
            for value in values {
                let value = value.as_str().expect("string enum");
                assert!(
                    !matches!(decode(value), ReviewState::Unknown(_)),
                    "{name} value {value:?} is not handled"
                );
                seen += 1;
            }
        }
        assert!(seen > 0, "no review-state schemas found in openapi.json");
    }
}
