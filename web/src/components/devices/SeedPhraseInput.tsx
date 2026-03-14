import { useCallback, useRef, useState } from "react";
import { cn } from "@/lib/cn";
import { FileTextIcon, WarningIcon } from "@phosphor-icons/react";

const WORD_COUNT = 24;
const ROWS = 6;

interface SeedPhraseInputProps {
  readonly onSubmit: (phrase: string) => void;
  readonly onCancel?: () => void;
  readonly loading?: boolean;
  readonly error?: string | null;
}

import { extractSeedPhraseWords } from "@/lib/seedPhraseParser";

export function SeedPhraseInput({ onSubmit, onCancel, loading, error }: SeedPhraseInputProps) {
  const [words, setWords] = useState<readonly string[]>(
    Array.from({ length: WORD_COUNT }, () => ""),
  );
  const inputRefs = useRef<(HTMLInputElement | null)[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const setWord = useCallback((index: number, value: string) => {
    setWords((prev) => prev.map((w, i) => (i === index ? value.toLowerCase().trim() : w)));
  }, []);

  const handlePaste = useCallback((index: number, e: React.ClipboardEvent<HTMLInputElement>) => {
    const text = e.clipboardData.getData("text");
    const extracted = extractSeedPhraseWords(text);

    if (extracted.length > 1) {
      e.preventDefault();
      setWords((prev) =>
        prev.map((w, i) => {
          const offset = i - index;
          return offset >= 0 && offset < extracted.length ? extracted[offset] : w;
        }),
      );

      const focusIdx = Math.min(index + extracted.length, WORD_COUNT - 1);
      inputRefs.current[focusIdx]?.focus();
    }
  }, []);

  const handleFileUpload = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    // eslint-disable-next-line functional/immutable-data -- FileReader API requires callback assignment
    reader.onload = () => {
      const text = reader.result as string;
      const extracted = extractSeedPhraseWords(text);
      setWords(
        Array.from({ length: WORD_COUNT }, (_, i) => (i < extracted.length ? extracted[i] : "")),
      );
    };
    reader.readAsText(file);

    // Reset so the same file can be re-selected.
    e.target.value = "";
  }, []);

  const handleKeyDown = useCallback(
    (index: number, e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === " ") {
        e.preventDefault();
        if (index < WORD_COUNT - 1) {
          inputRefs.current[index + 1]?.focus();
        }
      } else if (e.key === "Backspace" && words[index] === "" && index > 0) {
        inputRefs.current[index - 1]?.focus();
      } else if (e.key === "Enter") {
        const filled = words.filter((w) => w.length > 0).length;
        if (filled === WORD_COUNT) {
          onSubmit(words.join(" "));
        }
      }
    },
    [words, onSubmit],
  );

  const filledCount = words.filter((w) => w.length > 0).length;
  const canSubmit = filledCount === WORD_COUNT && !loading;

  return (
    <div className="flex flex-col items-center gap-6">
      <div className="text-center">
        <h2 className="text-base-content text-lg font-semibold">Enter your seed phrase</h2>
        <p className="text-base-content/60 mt-1 text-sm">
          Type or paste your 24 words below, or import from a .txt file.
        </p>
      </div>

      <div
        className="grid grid-cols-4 gap-x-4 gap-y-2"
        style={{ gridAutoFlow: "column", gridTemplateRows: `repeat(${ROWS}, auto)` }}
        role="group"
        aria-label="Seed phrase input"
      >
        {Array.from({ length: WORD_COUNT }, (_, idx) => (
          <div key={idx} className="flex items-center gap-1.5">
            <span className="text-base-content/40 w-6 text-right font-mono text-xs">
              {idx + 1}.
            </span>
            <input
              ref={(el) => {
                inputRefs.current[idx] = el;
              }}
              type="text"
              className={cn(
                "input input-bordered input-sm w-28 font-mono text-sm",
                words[idx] && "input-success",
              )}
              value={words[idx]}
              onChange={(e) => setWord(idx, e.target.value)}
              onPaste={(e) => handlePaste(idx, e)}
              onKeyDown={(e) => handleKeyDown(idx, e)}
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              aria-label={`Word ${idx + 1}`}
              disabled={loading}
            />
          </div>
        ))}
      </div>

      {error && (
        <div className="text-error flex items-center gap-2 text-sm" role="alert">
          <WarningIcon size={16} />
          {error}
        </div>
      )}

      <div className="flex flex-wrap items-center justify-center gap-3">
        <button
          onClick={() => fileInputRef.current?.click()}
          className="btn btn-outline btn-sm gap-2"
          disabled={loading}
        >
          <FileTextIcon size={16} />
          Import .txt
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept=".txt,text/plain"
          className="hidden"
          onChange={handleFileUpload}
          aria-hidden="true"
        />

        {onCancel && (
          <button onClick={onCancel} className="btn btn-ghost btn-sm" disabled={loading}>
            Cancel
          </button>
        )}

        <button
          onClick={() => onSubmit(words.join(" "))}
          disabled={!canSubmit}
          className={cn("btn btn-primary btn-sm", loading && "loading")}
        >
          {loading ? "Recovering..." : "Recover"}
        </button>
      </div>

      <p className="text-base-content/40 text-xs">
        {filledCount} of {WORD_COUNT} words entered
      </p>
    </div>
  );
}
