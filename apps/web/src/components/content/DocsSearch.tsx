import { useEffect, useState, useMemo, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { searchDocs, type SearchHit } from "@/lib/docs-search";
import { GROUP_META } from "@/lib/docs-registry";

/**
 * Command-palette search over the docs. Triggered by `Cmd+K` / `Ctrl+K` or
 * by clicking the sidebar search button. Searches doc titles, descriptions,
 * and `##`+ headings extracted at build time from each MDX file.
 *
 * cmdk handles keyboard nav (Up/Down/Enter) and focus trapping within the
 * palette. We just give it a list of candidates and a select handler.
 */

interface DocsSearchProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}

export function DocsSearch({ open, onOpenChange }: DocsSearchProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Reset the query whenever the palette toggles so reopening starts fresh.
  // Tracking the previous `open` and adjusting during render is React's
  // prescribed alternative to resetting derived state inside an effect.
  const [prevOpen, setPrevOpen] = useState(open);
  if (open !== prevOpen) {
    setPrevOpen(open);
    setQuery("");
  }

  // A command palette must take keyboard focus the moment it appears — the
  // input is only in the DOM once `open`, so focus is moved here rather than
  // via the (accessibility-flagged) autoFocus prop.
  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  // Global Cmd+K / Ctrl+K toggle. Ignore when a modifier besides the intended
  // meta/ctrl is active, to avoid intercepting browser shortcuts like Ctrl+Shift+K.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const isToggle =
        (event.metaKey || event.ctrlKey) &&
        !event.shiftKey &&
        !event.altKey &&
        event.key.toLowerCase() === "k";
      if (isToggle) {
        event.preventDefault();
        onOpenChange(!open);
      } else if (event.key === "Escape" && open) {
        onOpenChange(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onOpenChange]);

  const hits = useMemo(() => searchDocs(query), [query]);

  const onSelect = (hit: SearchHit) => {
    onOpenChange(false);
    // The href already includes the heading anchor when relevant.
    void navigate({ to: hit.href });
  };

  if (!open) return null;
  // SSR guard: `document` isn't defined during server rendering. The palette
  // is only ever opened via user interaction, so at that point we're on the
  // client and the body is mounted. Portaling escapes any transformed/
  // filtered ancestor that'd otherwise trap `position: fixed` to itself.
  if (typeof document === "undefined") return null;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-start justify-center p-4 pt-[10vh]">
      {/* Click-outside-to-close as a real button so it's keyboard-activatable;
          kept out of the tab order since Escape already closes the palette and
          cmdk traps focus within the dialog. */}
      <button
        type="button"
        aria-label="Close search"
        tabIndex={-1}
        onClick={() => onOpenChange(false)}
        className="absolute inset-0 cursor-default bg-black/30"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Search documentation"
        className="bg-base-100 border-border-accent/40 relative w-full max-w-xl overflow-hidden rounded-xl border shadow-panel-lg"
      >
        <Command
          label="Search documentation"
          shouldFilter={false}
          className="flex flex-col"
        >
          <div className="border-border-accent/30 flex items-center gap-2 border-b px-4 py-3">
            <MagnifyingGlassIcon size={16} aria-hidden="true" className="text-text-muted" />
            <Command.Input
              ref={inputRef}
              value={query}
              onValueChange={setQuery}
              placeholder="Search docs…"
              className="text-base-content placeholder:text-text-muted flex-1 bg-transparent text-[0.9rem] outline-none"
            />
            <kbd className="text-text-muted border-border-accent/40 rounded border px-1.5 py-0.5 text-[0.65rem]">
              Esc
            </kbd>
          </div>
          <Command.List className="max-h-[60vh] overflow-y-auto p-2">
            {query.trim() === "" ? (
              <div className="text-text-muted p-6 text-center text-[0.85rem]">
                Start typing to search chapters, sections, and descriptions.
              </div>
            ) : hits.length === 0 ? (
              <Command.Empty className="text-text-muted p-6 text-center text-[0.85rem]">
                No matches for "{query}".
              </Command.Empty>
            ) : (
              hits.map((hit, index) => (
                <Command.Item
                  key={`${hit.docGroup ?? "flat"}-${hit.docSlug}`}
                  value={`${hit.docTitle} ${hit.docDescription} ${index}`}
                  onSelect={() => onSelect(hit)}
                  className="data-[selected=true]:bg-accent/40 flex cursor-pointer flex-col gap-0.5 rounded-lg px-3 py-2 transition-colors"
                >
                  <div className="flex items-baseline gap-2">
                    <span className="text-base-content text-[0.9rem] font-medium">
                      {hit.docTitle}
                    </span>
                    {hit.docGroup && (
                      <span className="text-text-muted text-[0.7rem]">
                        {GROUP_META[hit.docGroup] ?? hit.docGroup}
                      </span>
                    )}
                  </div>
                  <span className="text-text-muted line-clamp-1 text-[0.75rem]">
                    {hit.docDescription}
                  </span>
                </Command.Item>
              ))
            )}
          </Command.List>
        </Command>
      </div>
    </div>,
    document.body,
  );
}

/**
 * Button that opens the search palette. Meant for the docs sidebar header.
 * Shows the current platform's Cmd/Ctrl shortcut hint.
 */
interface DocsSearchButtonProps {
  readonly onClick: () => void;
  readonly compact?: boolean;
}

export function DocsSearchButton({ onClick, compact = false }: DocsSearchButtonProps) {
  // Detect macOS to pick ⌘ vs Ctrl for the hint. Read through
  // useSyncExternalStore so the server renders "Ctrl" and the client swaps to
  // the real value on hydration without a mismatch — and without touching the
  // deprecated navigator.platform.
  const isMac = useSyncExternalStore(
    () => () => undefined,
    () => /mac/i.test(navigator.userAgent),
    () => false,
  );
  const modifierKey = isMac ? "⌘" : "Ctrl";

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="Search documentation (keyboard shortcut available)"
      aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
      className={`border-border-accent/40 bg-base-100 text-text-muted hover:border-primary/40 hover:text-base-content flex w-full items-center gap-2 rounded-lg border px-3 transition-colors ${
        compact ? "py-1.5 text-[0.72rem]" : "py-2 text-[0.82rem]"
      }`}
    >
      <MagnifyingGlassIcon size={compact ? 12 : 14} aria-hidden="true" />
      <span className="flex-1 text-left">Search docs</span>
      <kbd className="border-border-accent/40 rounded border px-1 py-0.5 text-[0.65rem]">
        {modifierKey}K
      </kbd>
    </button>
  );
}
