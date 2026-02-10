use chrono::{Duration, Weekday};

/// The booking window: how far before class time the booking opens (7 days + 2 hours)
pub fn booking_window() -> Duration {
    Duration::days(7) + Duration::hours(2)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_hours_mins_secs() {
        let d = chrono::Duration::hours(2) + chrono::Duration::minutes(30) + chrono::Duration::seconds(15);
        assert_eq!(format_duration(d), "2h 30m 15s");
    }

    #[test]
    fn format_duration_mins_secs() {
        let d = chrono::Duration::minutes(5) + chrono::Duration::seconds(42);
        assert_eq!(format_duration(d), "5m 42s");
    }

    #[test]
    fn format_duration_secs_only() {
        let d = chrono::Duration::seconds(7);
        assert_eq!(format_duration(d), "7s");
    }

    #[test]
    fn format_duration_zero() {
        let d = chrono::Duration::seconds(0);
        assert_eq!(format_duration(d), "0s");
    }

    #[test]
    fn truncate_short_string_noop() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_over_length() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn weekday_matches_full_names() {
        assert!(weekday_matches("monday", Weekday::Mon));
        assert!(weekday_matches("tuesday", Weekday::Tue));
        assert!(weekday_matches("wednesday", Weekday::Wed));
        assert!(weekday_matches("thursday", Weekday::Thu));
        assert!(weekday_matches("friday", Weekday::Fri));
        assert!(weekday_matches("saturday", Weekday::Sat));
        assert!(weekday_matches("sunday", Weekday::Sun));
    }

    #[test]
    fn weekday_matches_abbreviations() {
        assert!(weekday_matches("mon", Weekday::Mon));
        assert!(weekday_matches("tue", Weekday::Tue));
        assert!(weekday_matches("wed", Weekday::Wed));
        assert!(weekday_matches("thu", Weekday::Thu));
        assert!(weekday_matches("fri", Weekday::Fri));
        assert!(weekday_matches("sat", Weekday::Sat));
        assert!(weekday_matches("sun", Weekday::Sun));
    }

    #[test]
    fn weekday_matches_case_insensitive() {
        assert!(weekday_matches("Monday", Weekday::Mon));
        assert!(weekday_matches("FRIDAY", Weekday::Fri));
        assert!(weekday_matches("Wed", Weekday::Wed));
    }

    #[test]
    fn weekday_matches_non_match() {
        assert!(!weekday_matches("monday", Weekday::Tue));
        assert!(!weekday_matches("xyz", Weekday::Mon));
        assert!(!weekday_matches("", Weekday::Mon));
    }
}
