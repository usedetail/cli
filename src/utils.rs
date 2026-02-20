use chrono::Local;

/// Convert page number and limit to offset for pagination
pub fn page_to_offset(page: u32, limit: u32) -> u32 {
    (page - 1).saturating_mul(limit)
}

/// Conversion factor from milliseconds to seconds
const MS_TO_SECONDS: i64 = 1000;

/// Format a UTC timestamp (in milliseconds) in the machine's local timezone.
fn format_timestamp(timestamp_ms: i64, fmt: &str) -> String {
    chrono::DateTime::from_timestamp(timestamp_ms / MS_TO_SECONDS, 0)
        .map(|dt| dt.with_timezone(&Local).format(fmt).to_string())
        .unwrap_or_else(|| "-".into())
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
        chrono::DateTime::from_timestamp(timestamp_ms / MS_TO_SECONDS, 0)
            .expect("valid timestamp")
            .with_timezone(&chrono::Local)
            .format(fmt)
            .to_string()
    }

    #[test]
    fn page_to_offset_first_page() {
        assert_eq!(page_to_offset(1, 50), 0);
    }

    #[test]
    fn page_to_offset_second_page() {
        assert_eq!(page_to_offset(2, 50), 50);
    }

    #[test]
    fn page_to_offset_custom_limit() {
        assert_eq!(page_to_offset(3, 10), 20);
    }

    #[test]
    fn page_to_offset_limit_one() {
        assert_eq!(page_to_offset(5, 1), 4);
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
}
