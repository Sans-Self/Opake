import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

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
  readonly emptyLabel?: string;
  // Accessible name for the trigger. Required whenever `trigger` is an icon
  // with no text child — an icon-only button has no accessible name otherwise
  // and screen readers announce it as an anonymous "button".
  readonly triggerLabel?: string;
}

export function DropdownMenu({
  trigger,
  items,
  triggerClassName = "btn btn-neutral btn-sm gap-1.5 rounded-lg text-xs",
  align = "left",
  emptyLabel,
  triggerLabel,
}: DropdownMenuProps) {
  const [menuStyle, setMenuStyle] = useState<React.CSSProperties>({});
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLUListElement>(null);

  const close = useCallback(() => setOpen(false), []);

  const toggle = useCallback(() => {
    setOpen((prev) => {
      if (!prev) {
        const rect = triggerRef.current?.getBoundingClientRect();
        if (rect) {
          setMenuStyle({
            position: "fixed",
            top: rect.bottom + 4,
            ...(align === "left" ? { right: window.innerWidth - rect.right } : { left: rect.left }),
            zIndex: 9999,
          });
        }
      }
      return !prev;
    });
  }, [align]);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  return (
    <>
      <button
        ref={triggerRef}
        className={triggerClassName}
        onClick={toggle}
        aria-label={triggerLabel}
        aria-expanded={open}
        aria-haspopup="true"
      >
        {trigger}
      </button>
      {open &&
        createPortal(
          <ul
            ref={menuRef}
            className="menu border-base-300/50 bg-base-100 shadow-panel-lg w-42 rounded-xl border p-1"
            style={menuStyle}
          >
            {items.length === 0 && emptyLabel ? (
              <li className="text-caption text-text-faint px-2.5 py-2">{emptyLabel}</li>
            ) : (
              items.map(({ icon: Icon, label, onClick }) => (
                <li key={label}>
                  <button
                    onClick={() => {
                      close();
                      onClick?.();
                    }}
                    className="text-secondary gap-2.5 text-xs"
                  >
                    <Icon size={13} className="text-text-muted" />
                    {label}
                  </button>
                </li>
              ))
            )}
          </ul>,
          document.body,
        )}
    </>
  );
}
