use chrono::Weekday;

/// Format a duration as human-readable string (e.g., "2h 30m 15s")
pub fn format_duration(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Truncate a string to max_len characters, adding "..." if truncated
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Check if a day string matches a weekday
pub fn weekday_matches(day_str: &str, weekday: Weekday) -> bool {
    matches!(
        (day_str.to_lowercase().as_str(), weekday),
        ("monday" | "mon", Weekday::Mon)
            | ("tuesday" | "tue", Weekday::Tue)
            | ("wednesday" | "wed", Weekday::Wed)
            | ("thursday" | "thu", Weekday::Thu)
            | ("friday" | "fri", Weekday::Fri)
            | ("saturday" | "sat", Weekday::Sat)
            | ("sunday" | "sun", Weekday::Sun)
    )
}
