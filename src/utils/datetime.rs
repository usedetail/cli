use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Format a UTC timestamp (in milliseconds) in the machine's local timezone.
fn format_timestamp(timestamp_ms: i64, fmt: &str) -> String {
    // `from_timestamp_millis` floors toward negative infinity, so timestamps
    // in (-1000, 0) correctly land in the second before the epoch instead of
    // collapsing to epoch via integer-division truncation.
    DateTime::from_timestamp_millis(timestamp_ms).map_or_else(
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

/// Parse a `--since` / `--until` value into a UTC instant relative to `now`.
///
/// Accepted forms:
///   * Relative duration suffixed with `s|m|h|d|w` — e.g. `30s`, `15m`,
///     `24h`, `7d`, `2w`. Resolves to `now - duration`.
///   * RFC3339 timestamp — e.g. `2024-01-15T12:00:00Z`.
///   * `YYYY-MM-DD` — interpreted as midnight *local* time on that date, so
///     the window lines up with the dates the CLI prints (see `format_date`);
///     ambiguous or nonexistent local midnights fall back to midnight UTC.
pub fn parse_time_spec(s: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let trimmed = s.trim();
    if let Some(d) = parse_relative_duration(trimmed) {
        return now
            .checked_sub_signed(d)
            .ok_or_else(|| anyhow!("'{trimmed}' resolves to an out-of-range timestamp"));
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        if let Some(naive) = date.and_hms_opt(0, 0, 0) {
            // Anchor at *local* midnight, matching how `format_date` renders
            // the same timestamps; DST-gap dates fall back to UTC (see
            // `midnight_in_tz`).
            return Ok(midnight_in_tz(naive, &Local));
        }
    }
    Err(anyhow!(
        "could not parse '{trimmed}' as a duration (e.g. 1d, 24h), \
         a date (YYYY-MM-DD), or an RFC3339 timestamp"
    ))
}

/// Resolve a naive midnight into a UTC instant in the given time zone.
/// Zones without a unique local midnight on that date (DST transitions)
/// fall back to midnight UTC.
fn midnight_in_tz<Tz: TimeZone>(naive: NaiveDateTime, tz: &Tz) -> DateTime<Utc> {
    naive
        .and_local_timezone(tz)
        .single()
        .map_or(naive.and_utc(), |dt| dt.with_timezone(&Utc))
}

/// Parse `<n><unit>` where unit is one of `s|m|h|d|w` (case-insensitive).
/// Returns `None` when the input doesn't match — callers fall back to
/// absolute-date parsing.
fn parse_relative_duration(s: &str) -> Option<Duration> {
    let unit = s.chars().last()?;
    if !unit.is_ascii_alphabetic() {
        return None;
    }
    let n: i64 = s.strip_suffix(unit)?.trim().parse().ok()?;
    if n < 0 {
        return None;
    }
    match unit.to_ascii_lowercase() {
        's' => Duration::try_seconds(n),
        'm' => Duration::try_minutes(n),
        'h' => Duration::try_hours(n),
        'd' => Duration::try_days(n),
        'w' => Duration::try_weeks(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, MappedLocalTime, Offset};

    fn expected_local(timestamp_ms: i64, fmt: &str) -> String {
        DateTime::from_timestamp_millis(timestamp_ms)
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

    // ── parse_time_spec ──────────────────────────────────────────────

    fn fixed_now() -> DateTime<Utc> {
        // 2025-06-01 00:00:00 UTC
        DateTime::from_timestamp(1_748_736_000, 0).expect("valid timestamp")
    }

    #[test]
    fn parse_time_spec_seconds() {
        let now = fixed_now();
        let parsed = parse_time_spec("30s", now).expect("parses");
        assert_eq!((now - parsed).num_seconds(), 30);
    }

    #[test]
    fn parse_time_spec_minutes() {
        let now = fixed_now();
        let parsed = parse_time_spec("15m", now).expect("parses");
        assert_eq!((now - parsed).num_minutes(), 15);
    }

    #[test]
    fn parse_time_spec_hours() {
        let now = fixed_now();
        let parsed = parse_time_spec("24h", now).expect("parses");
        assert_eq!((now - parsed).num_hours(), 24);
    }

    #[test]
    fn parse_time_spec_days() {
        let now = fixed_now();
        let parsed = parse_time_spec("7d", now).expect("parses");
        assert_eq!((now - parsed).num_days(), 7);
    }

    #[test]
    fn parse_time_spec_weeks() {
        let now = fixed_now();
        let parsed = parse_time_spec("2w", now).expect("parses");
        assert_eq!((now - parsed).num_days(), 14);
    }

    #[test]
    fn parse_time_spec_unit_case_insensitive() {
        let now = fixed_now();
        let lower = parse_time_spec("3d", now).expect("parses");
        let upper = parse_time_spec("3D", now).expect("parses");
        assert_eq!(lower, upper);
    }

    #[test]
    fn parse_time_spec_zero_duration_is_now() {
        let now = fixed_now();
        let parsed = parse_time_spec("0d", now).expect("parses");
        assert_eq!(parsed, now);
    }

    /// A minimal time zone for exercising the DST fallback: offset fixed at
    /// +00:00, except local midnight on 2024-01-15 does not exist (as on DST
    /// spring-forward dates that skip 00:00).
    #[derive(Clone, Copy, Debug)]
    struct GapZone;

    #[derive(Clone, Copy, Debug)]
    struct GapZoneOffset(FixedOffset);

    impl Offset for GapZoneOffset {
        fn fix(&self) -> FixedOffset {
            self.0
        }
    }

    fn gap_offset() -> GapZoneOffset {
        GapZoneOffset(FixedOffset::east_opt(0).expect("valid offset"))
    }

    fn gap_midnight() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2024, 1, 15)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time")
    }

    impl TimeZone for GapZone {
        type Offset = GapZoneOffset;

        fn from_offset(_offset: &GapZoneOffset) -> GapZone {
            GapZone
        }

        fn offset_from_local_date(&self, _local: &NaiveDate) -> MappedLocalTime<GapZoneOffset> {
            MappedLocalTime::Single(gap_offset())
        }

        fn offset_from_local_datetime(
            &self,
            local: &NaiveDateTime,
        ) -> MappedLocalTime<GapZoneOffset> {
            if *local == gap_midnight() {
                MappedLocalTime::None
            } else {
                MappedLocalTime::Single(gap_offset())
            }
        }

        fn offset_from_utc_date(&self, _utc: &NaiveDate) -> GapZoneOffset {
            gap_offset()
        }

        fn offset_from_utc_datetime(&self, _utc: &NaiveDateTime) -> GapZoneOffset {
            gap_offset()
        }
    }

    #[test]
    fn parse_time_spec_iso_date() {
        let now = fixed_now();
        let parsed = parse_time_spec("2024-01-15", now).expect("parses");
        // Independent of `now`: the bound lands at 00:00 in the machine's
        // zone — the same wall clock the user sees in `format_date` output —
        // not at UTC midnight. On a UTC machine this coincides with UTC
        // midnight, so the anchoring itself is pinned by the `midnight_in_tz`
        // tests below, which use fixed non-UTC offsets and a DST-gap zone.
        assert_eq!(
            parsed
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            "2024-01-15 00:00:00"
        );
    }

    #[test]
    fn midnight_in_tz_fixed_offset_anchors_local_midnight() {
        // UTC-5: local midnight on 2024-01-15 is 2024-01-15T05:00:00Z — five
        // hours *after* the UTC midnight the old UTC-anchored parser returned.
        // A concrete instant, so the test discriminates the anchoring even on
        // a UTC CI runner.
        let tz = FixedOffset::west_opt(5 * 3600).expect("valid offset");
        let naive = NaiveDate::from_ymd_opt(2024, 1, 15)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time");
        assert_eq!(midnight_in_tz(naive, &tz).timestamp(), 1_705_294_800);
    }

    #[test]
    fn midnight_in_tz_missing_local_midnight_falls_back_to_utc() {
        // DST spring-forward gap: 00:00 local does not exist on this date, so
        // the bound falls back to midnight UTC (2024-01-15T00:00:00Z) instead
        // of panicking or guessing an offset.
        let naive = gap_midnight();
        assert_eq!(midnight_in_tz(naive, &GapZone).timestamp(), 1_705_276_800);
    }

    #[test]
    fn parse_time_spec_rfc3339() {
        let now = fixed_now();
        let parsed = parse_time_spec("2024-01-15T12:00:00Z", now).expect("parses");
        assert_eq!(parsed.timestamp(), 1_705_320_000);
    }

    #[test]
    fn parse_time_spec_rfc3339_with_offset_normalizes_to_utc() {
        let now = fixed_now();
        let with_offset = parse_time_spec("2024-01-15T12:00:00-05:00", now).expect("parses");
        let utc = parse_time_spec("2024-01-15T17:00:00Z", now).expect("parses");
        assert_eq!(with_offset, utc);
    }

    #[test]
    fn parse_time_spec_negative_duration_rejected() {
        let now = fixed_now();
        assert!(parse_time_spec("-3d", now).is_err());
    }

    #[test]
    fn parse_time_spec_unknown_unit_rejected() {
        let now = fixed_now();
        assert!(parse_time_spec("3x", now).is_err());
    }

    #[test]
    fn parse_time_spec_empty_rejected() {
        let now = fixed_now();
        assert!(parse_time_spec("", now).is_err());
    }

    #[test]
    fn parse_time_spec_unit_only_rejected() {
        let now = fixed_now();
        assert!(parse_time_spec("d", now).is_err());
    }

    #[test]
    fn parse_time_spec_trims_whitespace() {
        let now = fixed_now();
        let parsed = parse_time_spec("  1d  ", now).expect("parses");
        assert_eq!((now - parsed).num_days(), 1);
    }
}
