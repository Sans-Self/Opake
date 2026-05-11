// BIP-39 mnemonic generation from CSPRNG entropy.
//
// 256 bits entropy → SHA-256 checksum (first byte) → 264 bits total
// → split into 24 groups of 11 bits → index into 2048-word list.

use sha2::{Digest, Sha256};

use super::{wordlist, Mnemonic, BITS_PER_WORD, ENTROPY_BYTES, TOTAL_BITS, WORD_COUNT};
use crate::{CryptoRng, RngCore};

/// Generate a new 24-word BIP-39 mnemonic from 256 bits of CSPRNG entropy.
///
/// Infallible — the mnemonic is valid by construction.
pub fn generate_mnemonic(rng: &mut (impl CryptoRng + RngCore)) -> Mnemonic {
    let mut entropy = [0u8; ENTROPY_BYTES];
    rng.fill_bytes(&mut entropy);
    entropy_to_mnemonic(&entropy)
}

/// Encode raw entropy bytes into a BIP-39 mnemonic.
///
/// This is the core encoding logic, separated from RNG for testability —
/// we can feed known entropy to verify against test vectors.
pub(crate) fn entropy_to_mnemonic(entropy: &[u8; ENTROPY_BYTES]) -> Mnemonic {
    let list = wordlist();
    let checksum_byte = Sha256::digest(entropy)[0];

    // Build the full bitstream: 256 bits entropy + 8 bits checksum = 264 bits.
    let mut bits = vec![false; TOTAL_BITS];

    // Entropy bits.
    for (i, byte) in entropy.iter().enumerate() {
        for bit in 0..8 {
            bits[i * 8 + bit] = (byte >> (7 - bit)) & 1 == 1;
        }
    }

    // Checksum bits (first 8 bits of SHA-256 hash = full first byte for 256-bit entropy).
    for bit in 0..8 {
        bits[ENTROPY_BYTES * 8 + bit] = (checksum_byte >> (7 - bit)) & 1 == 1;
    }

    // Each 11-bit group selects a word.
    let words: Vec<String> = (0..WORD_COUNT)
        .map(|i| {
            let mut idx = 0usize;
            for bit in 0..BITS_PER_WORD {
                if bits[i * BITS_PER_WORD + bit] {
                    idx |= 1 << (BITS_PER_WORD - 1 - bit);
                }
            }
            list[idx].to_string()
        })
        .collect();

    Mnemonic { words }
}
