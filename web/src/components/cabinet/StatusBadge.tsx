import { Lock, Users, Globe } from "@phosphor-icons/react";
import type { EncStatus } from "./types";

const VARIANTS: Record<
  EncStatus,
  { className: string; icon: typeof Lock; label: string }
> = {
  private: {
    className: "badge-accent text-primary border-border-accent",
    icon: Lock,
    label: "Private",
  },
  shared: {
    className: "bg-bg-sage text-success border-success/30",
    icon: Users,
    label: "Shared",
  },
  public: {
    className: "bg-bg-stone text-text-muted border-base-300",
    icon: Globe,
    label: "Public",
  },
};

export function StatusBadge({ status }: { status: EncStatus }) {
  const variant = VARIANTS[status];
  const Icon = variant.icon;

  return (
    <span
      className={`badge badge-sm gap-1 border text-[10px] tracking-wide ${variant.className}`}
    >
      <Icon size={8} weight="bold" />
      {variant.label}
    </span>
  );
}
