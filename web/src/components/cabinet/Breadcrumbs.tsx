import type { ReactNode } from "react";

export function Breadcrumbs({ children }: { readonly children: ReactNode }) {
  return (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>{children}</ul>
    </div>
  );
}

export function BreadcrumbActive({ children }: { readonly children: ReactNode }) {
  return (
    <li>
      <span className="text-base-content font-medium">{children}</span>
    </li>
  );
}

export function BreadcrumbSkeleton() {
  return (
    <li>
      <span className="skeleton h-4 w-24 rounded" />
    </li>
  );
}
