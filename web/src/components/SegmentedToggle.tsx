import type { ComponentType } from "react";

interface ToggleOption<T extends string> {
  readonly value: T;
  readonly icon: ComponentType<{ readonly size: number }>;
}

interface SegmentedToggleProps<T extends string> {
  readonly options: readonly ToggleOption<T>[];
  readonly value: T;
  readonly onChange: (value: T) => void;
}

export function SegmentedToggle<T extends string>({
  options,
  value,
  onChange,
}: SegmentedToggleProps<T>) {
  return (
    <div className="join bg-primary/10 rounded-lg p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          onClick={() => onChange(option.value)}
          className={`join-item btn btn-xs rounded-md border-0 ${
            value === option.value
              ? "bg-base-100 text-secondary shadow-panel-sm"
              : "text-text-faint bg-transparent"
          }`}
        >
          <option.icon size={13} />
        </button>
      ))}
    </div>
  );
}
