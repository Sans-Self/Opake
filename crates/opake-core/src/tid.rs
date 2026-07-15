// TID (Timestamp ID) generation for AT Protocol record keys.
//
// A TID is a 13-character base32-sortable string encoding a microsecond
// timestamp. Used as rkeys for records that need client-generated IDs
// (e.g. for atomic applyWrites where the URI must be known before sending).
//
// Format: 64 bits → 13 chars of base32-sort alphabet
//   - bits 63..10: microseconds since Unix epoch (53 bits, ~285 years)
//   - bits 9..0:   clock ID (10 bits, 0 for single-process use)

const BASE32_SORT: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

/// Generate a TID from a microsecond timestamp.
///
/// The timestamp should be `SystemTime::now()` or equivalent in microseconds
/// since Unix epoch. Clock ID is set to 0 (single-process).
pub fn tid_from_micros(timestamp_micros: u64) -> String {
    let value = timestamp_micros << 10; // shift left to make room for clock ID (0)
    encode_base32_sort(value)
}

/// Generate a TID using the platform's clock.
///
/// Caller provides the current time in microseconds — injected for the same
/// reason as RNG (platform-agnostic core, no direct clock access).
pub fn tid_now(now_micros: fn() -> u64) -> String {
    tid_from_micros(now_micros())
}

/// Compute the AT-URI for a record that will be created with a given TID.
pub fn uri_with_tid(did: &str, collection: &str, tid: &str) -> String {
    format!("at://{did}/{collection}/{tid}")
}

fn encode_base32_sort(mut value: u64) -> String {
    let mut chars = [0u8; 13];
    for i in (0..13).rev() {
        chars[i] = BASE32_SORT[(value & 0x1f) as usize];
        value >>= 5;
    }
    // The encoding is always valid ASCII
    String::from_utf8(chars.to_vec()).expect("base32-sort is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tid_is_13_chars() {
        let tid = tid_from_micros(1_700_000_000_000_000);
        assert_eq!(tid.len(), 13);
    }

    #[test]
    fn tid_uses_base32_sort_alphabet() {
        let tid = tid_from_micros(1_700_000_000_000_000);
        assert!(
            tid.chars().all(|c| BASE32_SORT.contains(&(c as u8))),
            "invalid chars in TID: {tid}"
        );
    }

    #[test]
    fn tids_sort_chronologically() {
        let t1 = tid_from_micros(1_000_000);
        let t2 = tid_from_micros(2_000_000);
        let t3 = tid_from_micros(3_000_000);
        assert!(t1 < t2, "{t1} should sort before {t2}");
        assert!(t2 < t3, "{t2} should sort before {t3}");
    }

    #[test]
    fn zero_produces_all_twos() {
        let tid = tid_from_micros(0);
        assert_eq!(tid, "2222222222222");
    }

    #[test]
    fn uri_with_tid_formats_correctly() {
        let tid = tid_from_micros(1_700_000_000_000_000);
        let uri = uri_with_tid("did:plc:test", "at.opake.document", &tid);
        assert!(uri.starts_with("at://did:plc:test/at.opake.document/"));
        assert!(uri.ends_with(&tid));
    }

    #[test]
    fn different_timestamps_produce_different_tids() {
        let t1 = tid_from_micros(1_700_000_000_000_000);
        let t2 = tid_from_micros(1_700_000_000_000_001);
        assert_ne!(t1, t2);
    }
}
