//! Relative times shown in the interface ("hace 3 min").

/// Formats the time elapsed from `since_ms` to `now_ms` in Spanish.
pub fn ago(since_ms: i64, now_ms: i64) -> String {
    let seconds = ((now_ms - since_ms).max(0)) / 1000;
    match seconds {
        0..=9 => "ahora".to_string(),
        10..=59 => format!("hace {seconds} s"),
        60..=3599 => format!("hace {} min", seconds / 60),
        3600..=86_399 => format!("hace {} h", seconds / 3600),
        _ => format!("hace {} d", seconds / 86_400),
    }
}
