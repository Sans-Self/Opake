import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { Link, useMatchRoute } from "@tanstack/react-router";

interface SidebarItemProps {
  readonly to: string;
  readonly icon: PhosphorIcon;
  readonly label: string;
  readonly badge?: string | number;
}

export function SidebarItem({ to, icon: Icon, label, badge }: SidebarItemProps) {
  const matchRoute = useMatchRoute();
  const active = Boolean(matchRoute({ to, fuzzy: true }));

  return (
    <Link
      to={to}
      className={`text-ui flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.75 text-left transition-colors ${
        active ? "bg-accent text-primary" : "text-text-muted hover:bg-bg-hover"
      }`}
    >
      <Icon size={14} weight={active ? "fill" : "regular"} />
      <span className="flex-1">{label}</span>
      {badge !== undefined && (
        <span
          className={`badge badge-xs rounded-md ${
            active ? "bg-primary/20 text-primary" : "bg-primary/10 text-text-muted"
          }`}
        >
          {badge}
        </span>
      )}
    </Link>
  );
}
