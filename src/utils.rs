/// Convert page number and limit to offset for pagination
pub fn page_to_offset(page: u32, limit: u32) -> u32 {
    (page - 1) * limit
}

/// Conversion factor from milliseconds to seconds
const MS_TO_SECONDS: i64 = 1000;

/// Format a timestamp (in milliseconds) using the given strftime format string.
fn format_timestamp(timestamp_ms: i64, fmt: &str) -> String {
    chrono::DateTime::from_timestamp(timestamp_ms / MS_TO_SECONDS, 0)
        .map(|dt| dt.format(fmt).to_string())
        .unwrap_or_else(|| "-".into())
}

/// Format a timestamp (in milliseconds) as a date string (YYYY-MM-DD)
pub fn format_date(timestamp_ms: i64) -> String {
    format_timestamp(timestamp_ms, "%Y-%m-%d")
}

/// Format a timestamp (in milliseconds) as a full datetime string (YYYY-MM-DD HH:MM:SS UTC)
pub fn format_datetime(timestamp_ms: i64) -> String {
    format_timestamp(timestamp_ms, "%Y-%m-%d %H:%M:%S UTC")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 0 ms = 1970-01-01
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn format_date_known_timestamp() {
        // 2025-01-15 00:00:00 UTC in milliseconds
        assert_eq!(format_date(1_736_899_200_000), "2025-01-15");
    }

    #[test]
    fn format_date_negative_timestamp_returns_dash() {
        // Before epoch — chrono returns None for very large negative values
        assert_eq!(format_date(i64::MIN), "-");
    }

    #[test]
    fn format_datetime_unix_epoch() {
        assert_eq!(format_datetime(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn format_datetime_known_timestamp() {
        // 2025-01-15 13:30:00 UTC = 1_736_947_800_000 ms
        assert_eq!(
            format_datetime(1_736_947_800_000),
            "2025-01-15 13:30:00 UTC"
        );
    }

    #[test]
    fn format_datetime_rounds_down_sub_second() {
        // 500ms after epoch — sub-second component is dropped
        assert_eq!(format_datetime(500), "1970-01-01 00:00:00 UTC");
    }
}
