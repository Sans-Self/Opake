import { LockIcon, UsersIcon, GlobeIcon } from "@phosphor-icons/react"
import type { EncStatus } from "./types"

const VARIANTS: Readonly<
  Record<EncStatus, { className: string; icon: typeof LockIcon; label: string }>
> = {
  private: {
    className: "badge-accent text-primary border-border-accent",
    icon: LockIcon,
    label: "Private",
  },
  shared: {
    className: "bg-bg-sage text-success border-success/30",
    icon: UsersIcon,
    label: "Shared",
  },
  public: {
    className: "bg-bg-stone text-text-muted border-base-300",
    icon: GlobeIcon,
    label: "Public",
  },
}

export function StatusBadge({ status }: Readonly<{ status: EncStatus }>) {
  const variant = VARIANTS[status]
  const Icon = variant.icon

  return (
    <span className={`badge badge-sm text-label gap-1 border tracking-wide ${variant.className}`}>
      <Icon size={8} weight="bold" />
      {variant.label}
    </span>
  )
}
