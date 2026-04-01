// Platform-agnostic Unix timestamp and datetime parsing.
//
// Native: std::time::SystemTime. WASM: js_sys::Date.

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs() as i64
}

#[cfg(target_arch = "wasm32")]
pub fn unix_now() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_millis() as u64
}

#[cfg(target_arch = "wasm32")]
pub fn unix_now_millis() -> u64 {
    js_sys::Date::now() as u64
}

/// Parse an RFC 3339 / ISO 8601 datetime string to a Unix timestamp (seconds).
///
/// Handles the common atproto formats:
/// - `2026-03-01T12:00:00Z`
/// - `2026-03-01T12:00:00.123Z`
/// - `2026-03-01T12:00:00+00:00`
/// - `2026-03-01T12:00:00-05:00`
///
/// Returns `None` for unparseable input. No external dependencies.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    // Extract the UTC offset (in seconds) and strip it from the string.
    let (datetime, offset_seconds) = strip_offset(s)?;

    let (date, time) = datetime.split_once('T')?;

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let min: i64 = time_parts.next()?.parse().ok()?;
    let sec: i64 = time_parts.next()?.split('.').next()?.parse().ok()?;

    let days = days_from_civil(year, month, day);
    Some(days * 86400 + hour * 3600 + min * 60 + sec - offset_seconds)
}

/// Strip the timezone suffix and return (datetime_without_tz, offset_in_seconds).
fn strip_offset(s: &str) -> Option<(&str, i64)> {
    // "Z" = UTC
    if let Some(stripped) = s.strip_suffix('Z') {
        return Some((stripped, 0));
    }

    // "+HH:MM" or "-HH:MM" (always 6 chars from the end)
    if s.len() >= 6 {
        let (rest, tz) = s.split_at(s.len() - 6);
        let sign = tz.as_bytes()[0];
        if (sign == b'+' || sign == b'-') && tz.as_bytes()[3] == b':' {
            let hours: i64 = tz[1..3].parse().ok()?;
            let mins: i64 = tz[4..6].parse().ok()?;
            let offset = hours * 3600 + mins * 60;
            return Some((rest, if sign == b'+' { offset } else { -offset }));
        }
    }

    // No recognized suffix — assume UTC
    Some((s, 0))
}

/// Convert a civil date to a day count relative to the Unix epoch.
/// Algorithm: Howard Hinnant's `days_from_civil` (formally proven).
/// https://howardhinnant.github.io/date_algorithms.html#days_from_civil
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_z_suffix() {
        assert_eq!(parse_rfc3339("2026-03-01T12:00:00Z"), Some(1772366400));
    }

    #[test]
    fn parse_fractional_seconds() {
        assert_eq!(parse_rfc3339("2026-03-01T12:00:00.123Z"), Some(1772366400));
    }

    #[test]
    fn parse_positive_offset() {
        // 12:00:00+01:00 = 11:00:00 UTC
        assert_eq!(
            parse_rfc3339("2026-03-01T12:00:00+01:00"),
            Some(1772366400 - 3600)
        );
    }

    #[test]
    fn parse_negative_offset() {
        // 12:00:00-05:00 = 17:00:00 UTC
        assert_eq!(
            parse_rfc3339("2026-03-01T12:00:00-05:00"),
            Some(1772366400 + 5 * 3600)
        );
    }

    #[test]
    fn parse_zero_offset() {
        assert_eq!(parse_rfc3339("2026-03-01T12:00:00+00:00"), Some(1772366400));
    }

    #[test]
    fn unix_epoch() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn garbage_returns_none() {
        assert_eq!(parse_rfc3339("not a date"), None);
    }
}
