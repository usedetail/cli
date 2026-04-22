use chrono::Local;

/// Format a UTC timestamp (in milliseconds) in the machine's local timezone.
fn format_timestamp(timestamp_ms: i64, fmt: &str) -> String {
    // `from_timestamp_millis` floors toward negative infinity, so timestamps
    // in (-1000, 0) correctly land in the second before the epoch instead of
    // collapsing to epoch via integer-division truncation.
    chrono::DateTime::from_timestamp_millis(timestamp_ms).map_or_else(
        || "-".into(),
        |dt| dt.with_timezone(&Local).format(fmt).to_string(),
    )
}

/// Format a timestamp (in milliseconds) as a local date string (YYYY-MM-DD)
pub fn format_date(timestamp_ms: i64) -> String {
    format_timestamp(timestamp_ms, "%Y-%m-%d")
}

/// Format a timestamp (in milliseconds) as a local datetime string.
pub fn format_datetime(timestamp_ms: i64) -> String {
    format_timestamp(timestamp_ms, "%Y-%m-%d %H:%M:%S %Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_local(timestamp_ms: i64, fmt: &str) -> String {
        chrono::DateTime::from_timestamp_millis(timestamp_ms)
            .expect("valid timestamp")
            .with_timezone(&Local)
            .format(fmt)
            .to_string()
    }

    #[test]
    fn format_date_unix_epoch() {
        assert_eq!(format_date(0), expected_local(0, "%Y-%m-%d"));
    }

    #[test]
    fn format_date_known_timestamp() {
        assert_eq!(
            format_date(1_736_899_200_000),
            expected_local(1_736_899_200_000, "%Y-%m-%d")
        );
    }

    #[test]
    fn format_date_negative_timestamp_returns_dash() {
        // Before epoch — chrono returns None for very large negative values
        assert_eq!(format_date(i64::MIN), "-");
    }

    #[test]
    fn format_datetime_unix_epoch() {
        assert_eq!(
            format_datetime(0),
            expected_local(0, "%Y-%m-%d %H:%M:%S %Z")
        );
    }

    #[test]
    fn format_datetime_known_timestamp() {
        assert_eq!(
            format_datetime(1_736_947_800_000),
            expected_local(1_736_947_800_000, "%Y-%m-%d %H:%M:%S %Z")
        );
    }

    #[test]
    fn format_datetime_rounds_down_sub_second() {
        assert_eq!(
            format_datetime(500),
            expected_local(500, "%Y-%m-%d %H:%M:%S %Z")
        );
    }

    #[test]
    fn format_datetime_small_negative_is_not_epoch() {
        // Timestamps in (-1000, 0) ms should land in the second *before* the
        // epoch, not at the epoch itself. Integer-division truncation toward
        // zero previously collapsed this whole window onto 1970-01-01 00:00:00.
        assert_ne!(format_datetime(-1), format_datetime(0));
    }
}
