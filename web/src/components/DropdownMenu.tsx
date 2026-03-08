import { useEffect, useRef, type ComponentType, type ReactNode } from "react";

interface DropdownMenuItem {
  readonly icon: ComponentType<{ readonly size: number; readonly className?: string }>;
  readonly label: string;
  readonly onClick?: () => void;
}

interface DropdownMenuProps {
  readonly trigger: ReactNode;
  readonly items: readonly DropdownMenuItem[];
  readonly triggerClassName?: string;
  readonly align?: "left" | "right";
}

export function DropdownMenu({
  trigger,
  items,
  triggerClassName = "btn btn-neutral btn-sm gap-1.5 rounded-lg text-xs",
  align = "left",
}: DropdownMenuProps) {
  const detailsRef = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (detailsRef.current?.open && !detailsRef.current.contains(e.target as Node)) {
        detailsRef.current.removeAttribute("open");
      }
    };
    document.addEventListener("click", handleClickOutside);
    return () => document.removeEventListener("click", handleClickOutside);
  }, []);

  return (
    <details ref={detailsRef} className={`dropdown ${align === "left" ? "dropdown-end" : ""}`}>
      <summary className={triggerClassName}>{trigger}</summary>
      <ul className="menu dropdown-content border-base-300/50 bg-base-100 shadow-panel-lg z-[100] w-42 rounded-xl border p-1">
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
