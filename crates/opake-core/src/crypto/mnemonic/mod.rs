// BIP-39 mnemonic generation, validation, and key derivation.
//
// A 24-word mnemonic encodes 256 bits of entropy + 8-bit SHA-256 checksum.
// From those words we derive a 512-bit master seed via PBKDF2, then split
// it into X25519 (encryption) and Ed25519 (signing) keypairs via HKDF with
// domain-separated info strings.
//
// The `Mnemonic` type is the boundary validator — you can only construct one
// through `generate_mnemonic` (infallible, valid by construction) or
// `parse_mnemonic` (validates word count, wordlist membership, and checksum).
// This makes invalid mnemonics unrepresentable in the type system.

mod derive;
mod format;
mod generate;

use std::fmt;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::Error;

pub use derive::derive_identity_from_mnemonic;
pub use format::{format_mnemonic_grid, parse_mnemonic_grid};
pub use generate::generate_mnemonic;

// Embedded at compile time — works for both native and WASM.
const WORDLIST_RAW: &str = include_str!("../bip39_english.txt");

const WORD_COUNT: usize = 24;
const ENTROPY_BYTES: usize = 32; // 256 bits
const CHECKSUM_BITS: usize = 8; // ENT / 32 for 256-bit entropy
const BITS_PER_WORD: usize = 11; // log2(2048)
const TOTAL_BITS: usize = WORD_COUNT * BITS_PER_WORD; // 264

static WORDLIST: OnceLock<Vec<&'static str>> = OnceLock::new();

/// The cached BIP-39 English wordlist. Parsed once from the embedded string
/// on first access, then reused for all subsequent calls.
fn wordlist() -> &'static [&'static str] {
    WORDLIST.get_or_init(|| WORDLIST_RAW.lines().collect())
}

// ---------------------------------------------------------------------------
// Shared bit-packing helpers
// ---------------------------------------------------------------------------

/// Unpack word indices into a bitstream (11 bits per word).
fn indices_to_bits(indices: &[usize]) -> Vec<bool> {
    let mut bits = vec![false; TOTAL_BITS];
    for (i, &idx) in indices.iter().enumerate() {
        for bit in 0..BITS_PER_WORD {
            bits[i * BITS_PER_WORD + bit] = (idx >> (BITS_PER_WORD - 1 - bit)) & 1 == 1;
        }
    }
    bits
}

/// Extract entropy bytes from a bitstream (first 256 bits).
fn bits_to_entropy(bits: &[bool]) -> [u8; ENTROPY_BYTES] {
    let mut entropy = [0u8; ENTROPY_BYTES];
    for (i, byte) in entropy.iter_mut().enumerate() {
        for bit in 0..8 {
            if bits[i * 8 + bit] {
                *byte |= 1 << (7 - bit);
            }
        }
    }
    entropy
}

// ---------------------------------------------------------------------------
// Mnemonic type
// ---------------------------------------------------------------------------

/// A validated BIP-39 mnemonic (24 words, valid checksum).
///
/// Sensitive — contains the root secret from which all keys are derived.
/// Zeroized on drop so the words don't linger in memory.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Mnemonic {
    words: Vec<String>,
}

impl Mnemonic {
    /// The individual words of this mnemonic.
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// Recover the raw entropy bytes from this mnemonic. Used in tests
    /// to verify encoding roundtrips.
    #[cfg(test)]
    pub(crate) fn to_entropy(&self) -> Vec<u8> {
        let list = wordlist();
        let indices: Vec<usize> = self
            .words
            .iter()
            .map(|w| {
                list.binary_search(&w.as_str())
                    .expect("Mnemonic contains only validated words")
            })
            .collect();
        let bits = indices_to_bits(&indices);
        bits_to_entropy(&bits).to_vec()
    }
}

impl fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.words.join(" "))
    }
}

// Never print the phrase in logs.
impl fmt::Debug for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mnemonic([{} words])", self.words.len())
    }
}

// ---------------------------------------------------------------------------
// Parsing and validation
// ---------------------------------------------------------------------------

/// Parse a space-separated mnemonic phrase and validate it.
///
/// Checks: exactly 24 words, all words in BIP-39 English wordlist, SHA-256
/// checksum matches.
pub fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, Error> {
    let words: Vec<String> = phrase.split_whitespace().map(String::from).collect();

    if words.len() != WORD_COUNT {
        return Err(Error::Mnemonic(format!(
            "expected {WORD_COUNT} words, got {}",
            words.len()
        )));
    }

    // Validate all words and collect indices in a single pass.
    let list = wordlist();
    let mut indices = Vec::with_capacity(WORD_COUNT);
    for word in &words {
        match list.binary_search(&word.as_str()) {
            Ok(idx) => indices.push(idx),
            Err(_) => {
                return Err(Error::Mnemonic(format!(
                    "unknown word: {word:?} is not in the BIP-39 English wordlist"
                )));
            }
        }
    }

    // Verify checksum using the pre-computed indices.
    verify_checksum(&indices)?;

    Ok(Mnemonic { words })
}

/// Verify the checksum embedded in a mnemonic's word indices.
fn verify_checksum(indices: &[usize]) -> Result<(), Error> {
    let bits = indices_to_bits(indices);
    let entropy = bits_to_entropy(&bits);

    // Extract the checksum bits from the mnemonic (last 8 bits).
    let mut actual_checksum = 0u8;
    for bit in 0..CHECKSUM_BITS {
        if bits[ENTROPY_BYTES * 8 + bit] {
            actual_checksum |= 1 << (7 - bit);
        }
    }

    // Compute expected checksum: first byte of SHA-256(entropy).
    let expected_checksum = Sha256::digest(entropy)[0];

    if actual_checksum != expected_checksum {
        return Err(Error::Mnemonic(
            "invalid checksum — one or more words may be incorrect".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "../mnemonic_tests.rs"]
mod tests;
