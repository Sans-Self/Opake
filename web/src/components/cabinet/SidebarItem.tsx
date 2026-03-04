import type { Icon as PhosphorIcon } from "@phosphor-icons/react";

interface SidebarItemProps {
  icon: PhosphorIcon;
  label: string;
  active: boolean;
  badge?: string | number;
  onClick: () => void;
}

export function SidebarItem({
  icon: Icon,
  label,
  active,
  badge,
  onClick,
}: SidebarItemProps) {
  return (
    <button
      onClick={onClick}
      className={`flex w-full items-center gap-2.5 rounded-[9px] px-2.5 py-[7px] text-left text-[13px] transition-colors ${
        active
          ? "bg-accent text-primary"
          : "text-text-muted hover:bg-bg-hover"
      }`}
    >
      <Icon
        size={14}
        weight={active ? "fill" : "regular"}
      />
      <span className="flex-1">{label}</span>
      {badge !== undefined && (
        <span
          className={`rounded-[5px] px-1.5 py-px text-[10px] ${
            active
              ? "bg-primary/20 text-primary"
              : "bg-primary/10 text-text-muted"
          }`}
        >
          {badge}
        </span>
      )}
    </button>
  );
}
