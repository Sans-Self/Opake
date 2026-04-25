import { type ReactNode, useState, Children } from "react";

export function Breadcrumbs({ children }: { readonly children: ReactNode }) {
  const [expanded, setExpanded] = useState(false);
  const items = Children.toArray(children);
  const canCollapse = items.length > 2;

  return (
    <div className="breadcrumbs text-ui min-w-0 flex-1 overflow-hidden">
      <ul>
        {canCollapse && !expanded ? (
          <>
            {items[0]}
            <li>
              <button
                type="button"
                onClick={() => setExpanded(true)}
                className="text-text-faint hover:text-base-content transition-colors"
                aria-label="Show full path"
              >
                …
              </button>
            </li>
            {items[items.length - 1]}
          </>
        ) : (
          children
        )}
      </ul>
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
