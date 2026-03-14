import { describe, it, expect } from "vitest";
import { extractSeedPhraseWords } from "../../src/lib/seedPhraseParser";

// 24 distinct words for testing (valid BIP-39 words, but checksum doesn't matter here —
// we're testing the parser, not the mnemonic validator).
const WORDS_IN_ORDER = [
  "abandon", "ability", "able", "about", "above", "absent",
  "absorb", "abstract", "absurd", "abuse", "access", "accident",
  "account", "accuse", "achieve", "acid", "acoustic", "acquire",
  "across", "act", "action", "actor", "actress", "actual",
];

describe("extractSeedPhraseWords", () => {
  it("extracts plain space-separated words", () => {
    const result = extractSeedPhraseWords(WORDS_IN_ORDER.join(" "));
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("extracts newline-separated words", () => {
    const result = extractSeedPhraseWords(WORDS_IN_ORDER.join("\n"));
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("handles numbered list format (1. word per line)", () => {
    const text = WORDS_IN_ORDER.map((w, i) => `${i + 1}. ${w}`).join("\n");
    const result = extractSeedPhraseWords(text);
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("reorders column-major numbered grid to sequential", () => {
    // Simulate the 4×6 column-major grid format:
    // Row 0: "1. word1    7. word7   13. word13   19. word19"
    // Row 1: "2. word2    8. word8   14. word14   20. word20"
    // etc.
    const rows = Array.from({ length: 6 }, (_, row) =>
      Array.from({ length: 4 }, (_, col) => {
        const idx = col * 6 + row;
        return `${String(idx + 1).padStart(2)}. ${WORDS_IN_ORDER[idx]}`;
      }).join("  "),
    );
    const gridText = rows.join("\n");

    const result = extractSeedPhraseWords(gridText);
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("handles extra whitespace and blank lines", () => {
    const text = WORDS_IN_ORDER.map((w) => `  ${w}  `).join("\n\n");
    const result = extractSeedPhraseWords(text);
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("normalizes to lowercase", () => {
    const text = WORDS_IN_ORDER.map((w) => w.toUpperCase()).join(" ");
    const result = extractSeedPhraseWords(text);
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("strips non-alphabetic characters in plain mode", () => {
    const text = WORDS_IN_ORDER.join(", ");
    const result = extractSeedPhraseWords(text);
    expect(result).toEqual(WORDS_IN_ORDER);
  });

  it("falls back to left-to-right when fewer than 24 numbered pairs", () => {
    // Only 3 numbered entries — not enough for reordering, falls back.
    const text = "1. abandon 2. ability 3. able";
    const result = extractSeedPhraseWords(text);
    expect(result).toEqual(["abandon", "ability", "able"]);
  });

  it("handles mixed numbered and unnumbered (fallback)", () => {
    // Some numbered, some not — can't reliably reorder, so fallback.
    const text = "1. abandon ability 3. able";
    const result = extractSeedPhraseWords(text);
    // Falls back to plain extraction since < 24 numbered pairs.
    expect(result).toEqual(["abandon", "ability", "able"]);
  });

  it("returns empty array for empty input", () => {
    expect(extractSeedPhraseWords("")).toEqual([]);
  });

  it("returns empty array for numbers-only input", () => {
    expect(extractSeedPhraseWords("1 2 3 4")).toEqual([]);
  });
});
