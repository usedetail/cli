/// Convert page number and limit to offset for pagination
pub fn page_to_offset(page: u32, limit: u32) -> u32 {
    (page - 1) * limit
}

/// Conversion factor from milliseconds to seconds
const MS_TO_SECONDS: i64 = 1000;

/// Format a timestamp (in milliseconds) as a date string (YYYY-MM-DD)
pub fn format_date(timestamp_ms: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp_ms / MS_TO_SECONDS, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or("-".into())
}

/// Format a timestamp (in milliseconds) as a full datetime string (YYYY-MM-DD HH:MM:SS UTC)
pub fn format_datetime(timestamp_ms: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp_ms / MS_TO_SECONDS, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or("-".into())
}
