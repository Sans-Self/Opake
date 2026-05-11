use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

use super::generate::entropy_to_mnemonic;
use super::*;
use crate::{
    derive_keys_from_mnemonic, generate_mnemonic, parse_mnemonic, ML_KEM_PK_LEN, ML_KEM_SK_LEN,
};

/// Deterministic RNG seeded with zeros — produces known entropy for golden tests.
fn test_rng() -> ChaCha20Rng {
    ChaCha20Rng::from_seed([0u8; 32])
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

#[test]
fn generate_produces_24_words() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    assert_eq!(mnemonic.words().len(), 24);
}

#[test]
fn generate_roundtrips_through_parse() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let phrase = mnemonic.to_string();
    let parsed = parse_mnemonic(&phrase).expect("generated mnemonic should parse");
    assert_eq!(parsed.to_string(), phrase);
}

#[test]
fn generate_all_words_in_wordlist() {
    let list = wordlist();
    let mnemonic = generate_mnemonic(&mut test_rng());
    for word in mnemonic.words() {
        assert!(
            list.binary_search(&word.as_str()).is_ok(),
            "word {word:?} not in BIP-39 wordlist"
        );
    }
}

// ---------------------------------------------------------------------------
// Known-entropy encoding (BIP-39 spec conformance)
// ---------------------------------------------------------------------------

#[test]
fn known_entropy_produces_expected_words() {
    // All-zero entropy — a well-known BIP-39 test vector.
    // 256 zero bits → checksum = SHA256(0x00*32)[0] = 0x66
    // Expected: "abandon" x 23 + "art"
    let entropy = [0u8; 32];
    let mnemonic = entropy_to_mnemonic(&entropy);
    let words = mnemonic.words();
    assert_eq!(words.len(), 24);
    for word in &words[..23] {
        assert_eq!(word, "abandon");
    }
    assert_eq!(words[23], "art");
}

#[test]
fn all_ones_entropy_test_vector() {
    // All-0xFF entropy — another standard BIP-39 test vector.
    // SHA256(0xFF * 32)[0] = 0xAF → checksum bits = 10101111
    // Last word index: 0xFF top 3 bits (111) + checksum 8 bits (10101111) = 11111111010_1111
    // = 0b11111101011 = 2027 → "vote"
    // Expected: "zoo" x 23 + "vote"
    let entropy = [0xffu8; 32];
    let mnemonic = entropy_to_mnemonic(&entropy);
    let words = mnemonic.words();
    assert_eq!(words.len(), 24);
    for word in &words[..23] {
        assert_eq!(word, "zoo");
    }
    assert_eq!(words[23], "vote");
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn parse_rejects_wrong_word_count() {
    let err = parse_mnemonic("abandon ability able").unwrap_err();
    assert!(err.to_string().contains("expected 24 words"));
}

#[test]
fn parse_rejects_twelve_words() {
    let phrase = ["abandon"; 12].join(" ");
    let err = parse_mnemonic(&phrase).unwrap_err();
    assert!(err.to_string().contains("expected 24 words, got 12"));
}

#[test]
fn parse_rejects_unknown_word() {
    let mut words = vec!["abandon"; 23];
    words.push("notaword");
    let err = parse_mnemonic(&words.join(" ")).unwrap_err();
    assert!(err.to_string().contains("notaword"));
}

#[test]
fn parse_rejects_bad_checksum() {
    // Valid words but wrong checksum: "abandon" x 23 + "about" (should be "art").
    let mut words = vec!["abandon".to_string(); 23];
    words.push("about".to_string());
    let err = parse_mnemonic(&words.join(" ")).unwrap_err();
    assert!(err.to_string().contains("invalid checksum"));
}

#[test]
fn parse_accepts_valid_mnemonic() {
    // "abandon" x 23 + "art" is valid (all-zero entropy).
    let mut words = vec!["abandon"; 23];
    words.push("art");
    let result = parse_mnemonic(&words.join(" "));
    assert!(result.is_ok());
}

#[test]
fn parse_trims_whitespace() {
    let mut words = vec!["abandon"; 23];
    words.push("art");
    let phrase = format!("  {}  ", words.join("   "));
    let result = parse_mnemonic(&phrase);
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Entropy roundtrip
// ---------------------------------------------------------------------------

#[test]
fn entropy_roundtrips_through_mnemonic() {
    let original = [0u8; 32];
    let mnemonic = entropy_to_mnemonic(&original);
    let recovered = mnemonic.to_entropy();
    assert_eq!(recovered, original);
}

#[test]
fn nonzero_entropy_roundtrips() {
    let mut entropy = [0u8; 32];
    for (i, byte) in entropy.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let mnemonic = entropy_to_mnemonic(&entropy);
    let recovered = mnemonic.to_entropy();
    assert_eq!(recovered, entropy);
}

// ---------------------------------------------------------------------------
// Derivation
// ---------------------------------------------------------------------------

#[test]
fn derivation_is_deterministic() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let a = derive_keys_from_mnemonic(&mnemonic);
    let b = derive_keys_from_mnemonic(&mnemonic);
    assert_eq!(a.x25519_public, b.x25519_public);
    assert_eq!(a.x25519_private, b.x25519_private);
    assert_eq!(a.ed25519_signing, b.ed25519_signing);
    assert_eq!(a.ed25519_verifying, b.ed25519_verifying);
    assert_eq!(a.ml_kem_public, b.ml_kem_public);
    assert_eq!(a.ml_kem_private, b.ml_kem_private);
}

#[test]
fn derivation_produces_correct_key_lengths() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let secrets = derive_keys_from_mnemonic(&mnemonic);
    assert_eq!(secrets.x25519_public.len(), 32);
    assert_eq!(secrets.x25519_private.len(), 32);
    assert_eq!(secrets.ed25519_signing.len(), 32);
    assert_eq!(secrets.ed25519_verifying.len(), 32);
    assert_eq!(secrets.ml_kem_public.len(), ML_KEM_PK_LEN);
    assert_eq!(secrets.ml_kem_private.len(), ML_KEM_SK_LEN);
}

#[test]
fn different_mnemonics_produce_different_keys() {
    let m1 = generate_mnemonic(&mut ChaCha20Rng::from_seed([0u8; 32]));
    let m2 = generate_mnemonic(&mut ChaCha20Rng::from_seed([1u8; 32]));
    let a = derive_keys_from_mnemonic(&m1);
    let b = derive_keys_from_mnemonic(&m2);
    assert_ne!(a.x25519_public, b.x25519_public);
    assert_ne!(a.x25519_private, b.x25519_private);
}

// ---------------------------------------------------------------------------
// Golden test vector
// ---------------------------------------------------------------------------

#[test]
fn golden_vector_all_zero_entropy() {
    // All-zero entropy → known mnemonic → deterministic keys.
    // This test pins the entire derivation pipeline. If it breaks, the
    // derivation scheme changed and existing users can't recover keys.
    let entropy = [0u8; 32];
    let mnemonic = entropy_to_mnemonic(&entropy);
    assert_eq!(
        mnemonic.to_string(),
        "abandon abandon abandon abandon abandon abandon abandon abandon \
         abandon abandon abandon abandon abandon abandon abandon abandon \
         abandon abandon abandon abandon abandon abandon abandon art"
    );

    let secrets = derive_keys_from_mnemonic(&mnemonic);

    // Pinned base64 values. If these change, the derivation pipeline is
    // broken and existing seed-phrase-derived identities become
    // unrecoverable.
    assert_eq!(
        BASE64.encode(secrets.x25519_public),
        "7wIIxdbJBxTSFVOVTEdCV2//rOj/uvoiahBAvx8Ka1s="
    );
    assert_eq!(
        BASE64.encode(secrets.ed25519_verifying),
        "JsOAnxAptr3it1PIm0D5DNZdSdAsOfFmCHa2MXQg/AA="
    );
    // ML-KEM-768 public key fingerprint: first 32 bytes (44 base64 chars).
    // Anchors the hybrid-KEM derivation to a stable byte-level output;
    // 32 bytes is enough that any drift is caught with overwhelming probability.
    assert_eq!(
        &BASE64.encode(secrets.ml_kem_public)[..44],
        "zugaa5eck9IqFwgK4skuNnM0d4tpsfLNJ5c1XASw2VZh"
    );
}

// ---------------------------------------------------------------------------
// Debug redaction
// ---------------------------------------------------------------------------

#[test]
fn debug_does_not_leak_words() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let debug = format!("{mnemonic:?}");
    assert!(debug.contains("24 words"));
    assert!(!debug.contains("abandon"));
    // Make sure none of the actual words appear.
    for word in mnemonic.words() {
        assert!(!debug.contains(word.as_str()));
    }
}

// ---------------------------------------------------------------------------
// Wordlist sanity
// ---------------------------------------------------------------------------

#[test]
fn wordlist_has_2048_entries() {
    assert_eq!(wordlist().len(), 2048);
}

#[test]
fn wordlist_is_sorted() {
    let list = wordlist();
    for pair in list.windows(2) {
        assert!(
            pair[0] < pair[1],
            "wordlist not sorted: {:?} >= {:?}",
            pair[0],
            pair[1]
        );
    }
}

// ---------------------------------------------------------------------------
// Grid formatting
// ---------------------------------------------------------------------------

use crate::{format_mnemonic_grid, parse_mnemonic_grid};

#[test]
fn grid_roundtrips_through_parse() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let grid = format_mnemonic_grid(&mnemonic);
    let parsed = parse_mnemonic_grid(&grid).expect("grid should parse back");
    assert_eq!(parsed.to_string(), mnemonic.to_string());
}

#[test]
fn grid_has_correct_shape() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let grid = format_mnemonic_grid(&mnemonic);
    let lines: Vec<&str> = grid.lines().collect();
    assert_eq!(lines.len(), 6, "grid should have 6 rows");
    // Each line should contain 4 numbered words.
    for line in &lines {
        let numbers: Vec<&str> = line
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(numbers.len(), 4, "each row should have 4 numbers: {line}");
    }
}

#[test]
fn grid_numbers_are_1_through_24() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let grid = format_mnemonic_grid(&mnemonic);
    // Extract all numbers from the grid.
    let numbers: Vec<u32> = grid
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect();
    let mut sorted = numbers.clone();
    sorted.sort();
    assert_eq!(sorted, (1..=24).collect::<Vec<u32>>());
}

#[test]
fn grid_parse_lenient_newline_separated() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let one_per_line = mnemonic
        .words()
        .iter()
        .enumerate()
        .map(|(i, w)| format!("{}. {}", i + 1, w))
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = parse_mnemonic_grid(&one_per_line).expect("numbered list should parse");
    assert_eq!(parsed.to_string(), mnemonic.to_string());
}

#[test]
fn grid_parse_lenient_plain_words() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let plain = mnemonic.to_string();
    let parsed = parse_mnemonic_grid(&plain).expect("plain words should parse");
    assert_eq!(parsed.to_string(), mnemonic.to_string());
}

#[test]
fn grid_parse_lenient_extra_whitespace() {
    let mnemonic = generate_mnemonic(&mut test_rng());
    let messy = mnemonic
        .words()
        .iter()
        .map(|w| format!("  {w}  "))
        .collect::<Vec<_>>()
        .join("\n\n");
    let parsed = parse_mnemonic_grid(&messy).expect("messy whitespace should parse");
    assert_eq!(parsed.to_string(), mnemonic.to_string());
}

#[test]
fn grid_parse_rejects_invalid_words() {
    let err = parse_mnemonic_grid("1. notaword 2. abandon 3. abandon").unwrap_err();
    assert!(err.to_string().contains("notaword") || err.to_string().contains("expected 24"));
}

#[test]
fn grid_all_zero_entropy_roundtrip() {
    let entropy = [0u8; 32];
    let mnemonic = entropy_to_mnemonic(&entropy);
    let grid = format_mnemonic_grid(&mnemonic);
    let parsed = parse_mnemonic_grid(&grid).unwrap();
    assert_eq!(parsed.to_string(), mnemonic.to_string());
}
