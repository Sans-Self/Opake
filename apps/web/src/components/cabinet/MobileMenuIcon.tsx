// Animated hamburger icon — transforms to X when open.
// Adapted from BleeveNL/rel-cms.

import { cn } from "@/lib/cn";

interface MobileMenuIconProps {
  readonly open: boolean;
  readonly onClick?: () => void;
  readonly className?: string;
}

const lineClasses = "bg-base-content h-0.5 w-full origin-center transition-all relative";

export function MobileMenuIcon({ open, onClick, className }: MobileMenuIconProps) {
  return (
    <button
      onClick={onClick}
      className={cn("flex h-8 w-8 flex-col justify-around", className)}
      aria-label={open ? "Close menu" : "Open menu"}
      aria-expanded={open}
    >
      <span className={cn(lineClasses, open && "opacity-0")} />
      <span
        className={cn(
          lineClasses,
          "after:absolute after:inset-0 after:bg-inherit after:transition-all after:content-['']",
          open ? "rotate-45 after:-rotate-90" : "after:rotate-0",
        )}
      />
      <span className={cn(lineClasses, open && "opacity-0")} />
    </button>
  );
}
