import type { Icon } from "@phosphor-icons/react";

interface PageHeaderProps {
  readonly title: string;
  readonly description?: string;
  readonly icon?: Icon;
  readonly iconClassName?: string;
}

export function PageHeader({ title, description, icon: IconComponent, iconClassName }: PageHeaderProps) {
  return (
    <>
      {IconComponent && (
        <IconComponent size={48} className={iconClassName ?? "text-warning"} weight="fill" />
      )}
      <div className="flex flex-col gap-2">
        <h1 className="text-base-content text-2xl font-semibold">{title}</h1>
        {description && <p className="text-base-content/60 text-sm">{description}</p>}
      </div>
    </>
  );
}
