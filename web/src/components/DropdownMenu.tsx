import type { ComponentType, ReactNode } from "react";

interface DropdownMenuItem {
  readonly icon: ComponentType<{ readonly size: number; readonly className?: string }>;
  readonly label: string;
  readonly onClick?: () => void;
}

interface DropdownMenuProps {
  readonly trigger: ReactNode;
  readonly items: readonly DropdownMenuItem[];
}

export function DropdownMenu({ trigger, items }: DropdownMenuProps) {
  return (
    <details className="dropdown dropdown-end">
      <summary className="btn btn-neutral btn-sm gap-1.5 rounded-lg text-xs">{trigger}</summary>
      <ul className="menu dropdown-content border-base-300/50 bg-base-100 shadow-panel-lg z-50 w-42 rounded-xl border p-1">
        {items.map(({ icon: Icon, label, onClick }) => (
          <li key={label}>
            <button
              onClick={(e) => {
                e.currentTarget.closest("details")?.removeAttribute("open");
                onClick?.();
              }}
              className="text-secondary gap-2.5 text-xs"
            >
              <Icon size={13} className="text-text-muted" />
              {label}
            </button>
          </li>
        ))}
      </ul>
    </details>
  );
}
