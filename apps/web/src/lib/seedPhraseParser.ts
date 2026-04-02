// eslint-disable-next-line sonarjs/slow-regex -- no nested quantifiers, not vulnerable to backtracking
const NUMBERED_WORD_PATTERN = /(\d+)\.\s*([a-zA-Z]+)/g;

const WORD_COUNT = 24;

/** Extract words from text, handling both numbered grids and plain word lists.
 *  If the text contains numbered entries (e.g. "1. abandon  7. ability"),
 *  words are reordered by their numbers to handle column-major grid layouts. */
export function extractSeedPhraseWords(text: string): readonly string[] {
  // Try to extract (number, word) pairs for numbered formats.
  const pairs = [...text.matchAll(NUMBERED_WORD_PATTERN)].map((m) => ({
    num: parseInt(m[1], 10),
    word: m[2].toLowerCase(),
  }));

  // If we found 24 numbered words, sort by number for correct ordering.
  if (pairs.length === WORD_COUNT) {
    return [...pairs].sort((a, b) => a.num - b.num).map((p) => p.word);
  }

  // Fallback: strip non-alpha and take words left-to-right.
  return text
    .split(/[^a-zA-Z]+/)
    .filter((w) => w.length > 0)
    .map((w) => w.toLowerCase());
}
