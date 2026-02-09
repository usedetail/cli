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

/// Wrap text to fit within a maximum width, breaking on word boundaries
pub fn wrap_text(text: &str, max_width: usize) -> String {
    if text.len() <= max_width {
        return text.to_string();
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        // If adding this word would exceed max_width
        if !current_line.is_empty() && current_line.len() + word.len() + 1 > max_width {
            lines.push(current_line.clone());
            current_line.clear();
        }

        // If the word itself is longer than max_width, truncate it
        if word.len() > max_width {
            if !current_line.is_empty() {
                lines.push(current_line.clone());
                current_line.clear();
            }
            lines.push(format!("{}...", &word[..max_width.saturating_sub(3)]));
        } else {
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines.join("\n")
}

/// Wrap file paths to fit within a maximum width, breaking on slashes
pub fn wrap_path(path: &str, max_width: usize) -> String {
    if path.len() <= max_width {
        return path.to_string();
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    // Split on slashes but keep them
    for (i, segment) in path.split('/').enumerate() {
        let segment_with_slash = if i > 0 {
            format!("/{}", segment)
        } else {
            segment.to_string()
        };

        // If adding this segment would exceed max_width, start a new line
        if !current_line.is_empty() && current_line.len() + segment_with_slash.len() > max_width {
            lines.push(current_line.clone());
            current_line = segment_with_slash.trim_start_matches('/').to_string();
        } else {
            current_line.push_str(&segment_with_slash);
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines.join("/\n")
}
