// RFC 3339 timestamp formatting from microseconds since Unix epoch.
//
// Core deliberately has no chrono dependency — `Opake` injects the platform
// clock as `fn() -> u64` (microseconds) and derives the RFC 3339 string here.
// One injection instead of two, and the formatting is identical between CLI
// and WASM.
//
// Output shape: `YYYY-MM-DDTHH:MM:SS.ffffffZ` — microsecond precision, UTC,
// Z suffix. Accepted by atproto record schemas alongside any other valid
// ISO 8601 / RFC 3339 form.

/// Format microseconds since Unix epoch as an RFC 3339 UTC timestamp with
/// microsecond precision.
///
/// Only handles post-1970 timestamps (the input is unsigned). Returns
/// `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
pub fn rfc3339_from_micros(micros: u64) -> String {
    let seconds = micros / 1_000_000;
    let sub_micros = (micros % 1_000_000) as u32;

    let days = seconds / 86_400;
    let time_of_day = seconds % 86_400;
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let second = time_of_day % 60;

    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{sub_micros:06}Z"
    )
}

/// Convert days since 1970-01-01 to a civil (year, month, day).
///
/// Howard Hinnant's algorithm — see http://howardhinnant.github.io/date_algorithms.html.
/// Post-1970 only (input is `u64`, never pre-shift negative); leap years and
/// month lengths are handled by the mapping.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(rfc3339_from_micros(0), "1970-01-01T00:00:00.000000Z");
    }

    #[test]
    fn one_microsecond_past_epoch() {
        assert_eq!(rfc3339_from_micros(1), "1970-01-01T00:00:00.000001Z");
    }

    #[test]
    fn known_unix_timestamp_nov_2023() {
        // 1_700_000_000 s = 2023-11-14T22:13:20 UTC
        assert_eq!(
            rfc3339_from_micros(1_700_000_000_000_000),
            "2023-11-14T22:13:20.000000Z"
        );
    }

    #[test]
    fn leap_day_2024() {
        // 2024-02-29T00:00:00 UTC = 1_709_164_800 s
        assert_eq!(
            rfc3339_from_micros(1_709_164_800_000_000),
            "2024-02-29T00:00:00.000000Z"
        );
    }

    #[test]
    fn non_leap_century_2100() {
        // 2100 is NOT a leap year (divisible by 100 but not 400).
        // 2100-03-01T00:00:00 UTC = 4_107_542_400 s.
        // Check 2100-02-28 rolls to 03-01 without hitting a 02-29.
        assert_eq!(
            rfc3339_from_micros(4_107_542_400_000_000),
            "2100-03-01T00:00:00.000000Z"
        );
    }

    #[test]
    fn leap_year_2000_was_leap() {
        // 2000 IS a leap year (divisible by 400).
        // 2000-02-29T12:34:56 UTC = 951_827_696 s.
        assert_eq!(
            rfc3339_from_micros(951_827_696_000_000),
            "2000-02-29T12:34:56.000000Z"
        );
    }

    #[test]
    fn sub_second_precision_preserved() {
        // 1_700_000_000.123456 s
        assert_eq!(
            rfc3339_from_micros(1_700_000_000_123_456),
            "2023-11-14T22:13:20.123456Z"
        );
    }

    #[test]
    fn sub_second_pads_to_six_digits() {
        // Single-digit microseconds should be zero-padded.
        assert_eq!(
            rfc3339_from_micros(1_700_000_000_000_007),
            "2023-11-14T22:13:20.000007Z"
        );
    }

    #[test]
    fn output_is_sortable_lexicographically() {
        // The whole point of ISO 8601 with zero-padded fields: string order
        // matches chronological order.
        let earlier = rfc3339_from_micros(1_700_000_000_000_000);
        let later = rfc3339_from_micros(1_700_000_000_000_001);
        assert!(earlier < later);

        let much_later = rfc3339_from_micros(1_800_000_000_000_000);
        assert!(later < much_later);
    }
}
