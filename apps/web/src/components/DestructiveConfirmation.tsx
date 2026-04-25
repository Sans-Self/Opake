import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/cn";

interface DestructiveConfirmationProps {
  readonly phrase: string;
  readonly onConfirm: () => void;
  readonly className?: string;
}

function blockClipboardAndDrag(e: React.ClipboardEvent | React.DragEvent) {
  e.preventDefault();
}

export function DestructiveConfirmation({
  phrase,
  onConfirm,
  className,
}: DestructiveConfirmationProps) {
  const [typed, setTyped] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const confirmedRef = useRef(false);

  const hasError = typed.length > 0 && !phrase.startsWith(typed);
  const errorIndex = hasError ? typed.split("").findIndex((char, i) => char !== phrase[i]) : -1;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const value = e.target.value;
      if (value.length <= phrase.length) {
        setTyped(value);
      }
    },
    [phrase],
  );

  useEffect(() => {
    if (typed === phrase && !confirmedRef.current) {
      confirmedRef.current = true;
      onConfirm();
    }
  }, [typed, phrase, onConfirm]);

  return (
    <div
      className={cn(
        "card card-bordered bg-base-100 flex flex-col items-center gap-3 p-5",
        className,
      )}
    >
      <div
        className="relative cursor-text text-left"
        onClick={() => inputRef.current?.focus()}
        role="presentation"
      >
        {/* Ghost text — the full phrase, unselectable */}
        <div
          aria-hidden="true"
          className="text-base-content/20 pointer-events-none font-mono text-sm tracking-wide select-none"
        >
          {phrase.split("").map((char, i) => (
            <span
              key={i}
              className={cn(
                i < typed.length && "invisible",
                i === typed.length && "text-base-content/35",
              )}
            >
              {char}
            </span>
          ))}
        </div>

        {/* Typed overlay — sits on top, character-colored */}
        <div className="pointer-events-none absolute inset-0 font-mono text-sm tracking-wide">
          {typed.split("").map((char, i) => {
            const correct = char === phrase[i];
            return (
              <span
                key={i}
                className={cn(
                  correct && "text-base-content",
                  !correct && "text-error bg-error/10 rounded-sm",
                )}
              >
                {char}
              </span>
            );
          })}
          {/* Blinking caret */}
          {typed !== phrase && (
            <span className="border-primary/60 inline-block h-[1.1em] w-0 translate-y-[0.15em] animate-pulse border-l-2" />
          )}
        </div>

        {/* Invisible input for actual typing */}
        <input
          ref={inputRef}
          type="text"
          value={typed}
          onChange={handleChange}
          onPaste={blockClipboardAndDrag}
          onCopy={blockClipboardAndDrag}
          onCut={blockClipboardAndDrag}
          onDrop={blockClipboardAndDrag}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck={false}
          aria-label={`Type "${phrase}" to confirm`}
          className="absolute inset-0 cursor-text caret-transparent opacity-0"
        />
      </div>

      <p
        className={cn(
          "text-caption",
          hasError && errorIndex >= 0 ? "text-error" : "text-base-content/40",
        )}
        role="alert"
      >
        {hasError && errorIndex >= 0
          ? `Expected "${phrase[errorIndex]}" — try again from there`
          : "Type the phrase above to confirm"}
      </p>
    </div>
  );
}
