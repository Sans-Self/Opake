import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  CopySimpleIcon,
  DownloadSimpleIcon,
  CheckCircleIcon,
  CheckIcon,
} from "@phosphor-icons/react";

const COLUMNS = 4;
const ROWS = 6;

type Phase = "display" | "confirm" | "done";

interface SeedPhraseDisplayProps {
  readonly phrase: string;
  readonly onConfirmed: () => void;
}

/** Pick 3 distinct random indices from 0..23. */
function pickConfirmationIndices(): readonly number[] {
  const pool = Array.from({ length: 24 }, (_, i) => i);
  const picked = pool
    // eslint-disable-next-line sonarjs/pseudo-random -- not crypto, just UI word positions
    .map((val) => ({ val, sort: Math.random() }))
    .sort((a, b) => a.sort - b.sort)
    .slice(0, 3)
    .map((x) => x.val)
    .sort((a, b) => a - b);
  return picked;
}

/** Format the mnemonic as a downloadable .txt grid matching the CLI format. */
function formatGrid(words: readonly string[]): string {
  return (
    Array.from({ length: ROWS }, (_, row) =>
      Array.from({ length: COLUMNS }, (_, col) => {
        const idx = col * ROWS + row;
        const num = String(idx + 1).padStart(2, " ");
        const entry = `${num}. ${words[idx]}`;
        return col < COLUMNS - 1 ? entry.padEnd(18, " ") : entry;
      }).join(""),
    ).join("\n") + "\n"
  );
}

export function SeedPhraseDisplay({ phrase, onConfirmed }: SeedPhraseDisplayProps) {
  const words = useMemo(() => phrase.split(" "), [phrase]);
  const [phase, setPhase] = useState<Phase>("display");
  const [written, setWritten] = useState(false);
  const confirmIndices = useMemo(() => pickConfirmationIndices(), []);
  const [confirmInputs, setConfirmInputs] = useState<Readonly<Record<number, string>>>({});
  const [confirmError, setConfirmError] = useState<string | null>(null);

  const [copied, setCopied] = useState(false);
  const copyTimeoutRef = useRef<ReturnType<typeof setTimeout>>(null);

  useEffect(
    () => () => {
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current);
    },
    [],
  );

  const handleCopy = useCallback(() => {
    const text = words.map((w, i) => `${i + 1}. ${w}`).join("\n");
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      if (copyTimeoutRef.current) clearTimeout(copyTimeoutRef.current);
      copyTimeoutRef.current = setTimeout(() => setCopied(false), 2000);
    });
  }, [words]);

  const handleDownload = useCallback(() => {
    const blob = new Blob([formatGrid(words)], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    Object.assign(document.createElement("a"), {
      href: url,
      download: "opake-seed-phrase.txt",
    }).click();
    URL.revokeObjectURL(url);
  }, [words]);

  const handleConfirmSubmit = useCallback(() => {
    const firstWrong = confirmIndices.find(
      (idx) => (confirmInputs[idx] ?? "").trim().toLowerCase() !== words[idx],
    );
    if (firstWrong !== undefined) {
      setConfirmError(`Word #${firstWrong + 1} is incorrect.`);
      return;
    }
    setConfirmError(null);
    setPhase("done");
    onConfirmed();
  }, [confirmIndices, confirmInputs, words, onConfirmed]);

  if (phase === "display") {
    return (
      <div className="flex flex-col items-center gap-6">
        <div className="max-w-lg text-center">
          <h2 className="text-base-content text-lg font-semibold">Your secret words</h2>
          <p className="mt-1 text-base">
            Your secret key is based on these 24 words. These can be used as recovery key in the
            worst-case scenario where you lose access to all devices you have signed in to.
          </p>
          <p className="text-base-content/60 mt-1 text-sm">
            Write these words down. They will not be shown again.
          </p>
        </div>

        <div
          className="bg-base-200 grid grid-cols-4 gap-x-6 gap-y-2 rounded-lg p-5 font-mono text-sm select-none"
          role="list"
          aria-label="Seed phrase words"
        >
          {Array.from({ length: ROWS }, (_, row) =>
            Array.from({ length: COLUMNS }, (_, col) => {
              const idx = col * ROWS + row;
              return (
                <div key={idx} className="flex gap-2" role="listitem">
                  <span className="text-base-content/40 w-6 text-right">{idx + 1}.</span>
                  <span className="text-base-content">{words[idx]}</span>
                </div>
              );
            }),
          )}
        </div>

        <div className="flex flex-wrap items-center justify-center gap-3">
          <button onClick={handleCopy} className="btn btn-outline btn-sm gap-2">
            {copied ? <CheckIcon size={16} /> : <CopySimpleIcon size={16} />}
            {copied ? "Copied" : "Copy to clipboard"}
          </button>
          <button onClick={handleDownload} className="btn btn-outline btn-sm gap-2">
            <DownloadSimpleIcon size={16} />
            Download as .txt file
          </button>
        </div>

        <label className="flex cursor-pointer items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="checkbox checkbox-sm"
            checked={written}
            onChange={(e) => setWritten(e.target.checked)}
          />
          <span className="text-base-content/70">I have written down my seed phrase</span>
        </label>

        <button
          disabled={!written}
          onClick={() => setPhase("confirm")}
          className="btn btn-primary btn-sm"
        >
          Continue
        </button>
      </div>
    );
  }

  if (phase === "confirm") {
    return (
      <div className="flex flex-col items-center gap-6">
        <div className="text-center">
          <h2 className="text-base-content text-lg font-semibold">Confirm your seed phrase</h2>
          <p className="text-base-content/60 mt-1 text-sm">
            Enter the requested words to verify you saved them correctly.
          </p>
        </div>

        <div className="flex flex-col gap-4">
          {confirmIndices.map((idx) => (
            <label key={idx} className="flex items-center gap-3">
              <span className="text-base-content/60 w-20 shrink-0 text-right text-sm whitespace-nowrap">
                Word #{idx + 1}
              </span>
              <input
                type="text"
                className="input input-bordered input-sm w-40"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                value={confirmInputs[idx] ?? ""}
                onChange={(e) => setConfirmInputs((prev) => ({ ...prev, [idx]: e.target.value }))}
                onKeyDown={(e) => e.key === "Enter" && handleConfirmSubmit()}
              />
            </label>
          ))}
        </div>

        {confirmError && (
          <p className="text-error text-sm" role="alert">
            {confirmError}
          </p>
        )}

        <div className="flex gap-3">
          <button onClick={() => setPhase("display")} className="btn btn-ghost btn-sm">
            Back
          </button>
          <button onClick={handleConfirmSubmit} className="btn btn-primary btn-sm">
            Confirm
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-4">
      <CheckCircleIcon size={48} className="text-success" weight="fill" />
      <p className="text-base-content text-lg font-semibold">Seed phrase confirmed</p>
      <p className="text-base-content/60 text-sm">Setting up your encryption key...</p>
    </div>
  );
}
