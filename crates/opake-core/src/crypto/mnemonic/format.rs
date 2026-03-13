// Seed phrase grid formatting and lenient parsing.
//
// The grid layout is 4 columns × 6 rows, numbered:
//
//   1. abandon    7. abandon   13. abandon   19. abandon
//   2. ability    8. ability   14. ability   20. ability
//   ...
//
// The parser is deliberately lenient — it strips numbers, punctuation, and
// extra whitespace so it handles the numbered grid format, plain lists,
// newline-separated words, or any reasonable mix a user might paste from
// a .txt backup.

use std::fmt::Write;

use super::{parse_mnemonic, Mnemonic, WORD_COUNT};
use crate::error::Error;

const COLUMNS: usize = 4;
const ROWS: usize = 6; // WORD_COUNT / COLUMNS

/// Format a mnemonic as a numbered 4-column × 6-row grid.
///
/// Output is suitable for writing to a .txt backup file:
/// ```text
///  1. abandon    7. abandon   13. abandon   19. abandon
///  2. ability    8. ability   14. ability   20. ability
/// ```
pub fn format_mnemonic_grid(mnemonic: &Mnemonic) -> String {
    let words = mnemonic.words();
    let mut output = String::new();

    for row in 0..ROWS {
        for col in 0..COLUMNS {
            let word_index = col * ROWS + row;
            let number = word_index + 1;
            let word = &words[word_index];

            // Right-align the number, pad the entry to a fixed column width.
            // "13. abandon  " = number(1-2 chars) + ". " + word(up to ~8 chars)
            let entry = format!("{number:>2}. {word}");

            if col < COLUMNS - 1 {
                // Pad to 18 chars for alignment (covers longest BIP-39 word + number).
                let _ = write!(output, "{entry:<18}");
            } else {
                let _ = write!(output, "{entry}");
            }
        }
        output.push('\n');
    }

    output
}

/// Parse a seed phrase from a .txt backup, tolerating various formats.
///
/// Strips line numbers, dots, and other punctuation — extracts only
/// alphabetic words. Handles:
/// - Numbered grid (`1. abandon  7. abandon ...`)
/// - Plain space-separated (`abandon abandon abandon ...`)
/// - Newline-separated (one word per line)
/// - Numbered list (`1. abandon\n2. ability\n...`)
/// - Any mix of the above
///
/// When numbers are present, they're used to determine word order (so a
/// column-major grid like "1. word  7. word  13. word  19. word" is
/// reordered correctly). Without numbers, words are taken left-to-right.
pub fn parse_mnemonic_grid(text: &str) -> Result<Mnemonic, Error> {
    // Try to extract (number, word) pairs first.
    let numbered_pairs = extract_numbered_pairs(text);

    let phrase = if numbered_pairs.len() == WORD_COUNT {
        // All 24 words have numbers — sort by number for correct ordering.
        let mut pairs = numbered_pairs;
        pairs.sort_by_key(|(n, _)| *n);
        pairs
            .into_iter()
            .map(|(_, w)| w)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        // No numbers or incomplete numbering — take words left-to-right.
        text.split(|c: char| !c.is_alphabetic())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };

    parse_mnemonic(&phrase)
}

/// Extract (number, word) pairs from text like "1. abandon  7. ability".
/// Looks for patterns of `<digits>` followed by optional `.` then a word.
fn extract_numbered_pairs(text: &str) -> Vec<(usize, String)> {
    let mut pairs = Vec::new();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];

        // Try to parse "N." or "N" as a number prefix.
        let stripped = token.trim_end_matches('.');
        if let Ok(num) = stripped.parse::<usize>() {
            // Check if there's a word glued after the number+dot (e.g. "1.abandon").
            let suffix: String = token
                .chars()
                .skip_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();

            if !suffix.is_empty() && suffix.chars().all(|c| c.is_alphabetic()) {
                pairs.push((num, suffix));
            } else if i + 1 < tokens.len() {
                // "1. abandon" or "1 abandon" — word is the next token.
                let word = tokens[i + 1].trim_matches(|c: char| !c.is_alphabetic());
                if !word.is_empty() && word.chars().all(|c| c.is_alphabetic()) {
                    pairs.push((num, word.to_string()));
                    i += 1;
                }
            }
        }
        i += 1;
    }

    pairs
}
