//! Turning numbers into something a person reads.
//!
//! Unit formatting lives in the pack rather than in the vocabulary, because
//! how a byte count is written is presentation, and presentation is what a
//! design pack owns.

use chrono::{DateTime, Utc};

/// A value with its unit, written the way the unit implies.
pub fn value(value: f64, unit: Option<&str>) -> String {
    match unit {
        Some("bytes") => bytes(value),
        Some("bytes_per_second") => format!("{}/s", bytes(value)),
        Some("percent") => format!("{}%", trim(value)),
        Some("seconds") => duration(value),
        Some("milliseconds") => duration(value / 1000.0),
        Some("per_second") => format!("{}/s", trim(value)),
        Some("count") | None => trim(value),
        Some(other) => format!("{} {other}", trim(value)),
    }
}

/// A number without trailing noise: whole numbers plain, fractions to one
/// place.
pub fn trim(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.1}")
    }
}

fn bytes(value: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = value;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{} {}", trim(size), UNITS[unit])
}

fn duration(seconds: f64) -> String {
    if seconds < 1.0 {
        return format!("{}ms", trim(seconds * 1000.0));
    }
    if seconds < 60.0 {
        return format!("{}s", trim(seconds));
    }
    if seconds < 3600.0 {
        return format!("{}m", trim(seconds / 60.0));
    }
    format!("{}h", trim(seconds / 3600.0))
}

/// How long ago something was, for a stale panel's caption.
pub fn age(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s ago")
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

/// An instant, written for a table cell.
pub fn instant(at: &DateTime<Utc>) -> String {
    at.format("%H:%M:%S").to_string()
}

/// The change between two values, as a person would say it.
pub fn delta(current: f64, previous: f64) -> Option<String> {
    if previous.abs() < f64::EPSILON {
        return None;
    }
    let change = (current - previous) / previous * 100.0;
    if change.abs() < 0.05 {
        return Some("no change".to_string());
    }
    Some(format!(
        "{}{:.1}% vs previous",
        if change > 0.0 { "+" } else { "" },
        change
    ))
}
