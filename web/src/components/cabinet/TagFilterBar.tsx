import { XIcon } from "@phosphor-icons/react";

interface TagFilterBarProps {
  readonly availableTags: readonly string[];
  readonly activeFilters: readonly string[];
  readonly onToggle: (tag: string) => void;
  readonly onClear: () => void;
}

export function TagFilterBar({
  availableTags,
  activeFilters,
  onToggle,
  onClear,
}: TagFilterBarProps) {
  if (availableTags.length === 0) return null;

  const hasActive = activeFilters.length > 0;

  return (
    <div className="flex items-center gap-1.5 overflow-x-auto px-4 py-2 [scrollbar-width:none]">
      {hasActive && (
        <button
          onClick={onClear}
          className="btn btn-ghost btn-xs text-text-faint gap-1 rounded-full"
        >
          <XIcon size={10} />
          Clear
        </button>
      )}
      {availableTags.map((tag) => {
        const isActive = activeFilters.includes(tag);
        return (
          <button
            key={tag}
            onClick={() => onToggle(tag)}
            className={`btn btn-xs rounded-full ${
              isActive ? "btn-primary" : "btn-ghost border-base-300/50 border"
            }`}
          >
            {tag}
          </button>
        );
      })}
    </div>
  );
}
