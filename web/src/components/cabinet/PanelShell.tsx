import type { ReactNode } from "react";
import { ShieldCheckIcon } from "@phosphor-icons/react";

interface PanelShellProps {
  readonly depth: number;
  readonly breadcrumbs: ReactNode;
  readonly toolbar?: ReactNode;
  readonly footer: string;
  readonly children: ReactNode;
  /** When set, a second panel renders alongside the main one. */
  readonly sidePanel?: ReactNode;
}

const PANEL_CHROME =
  "border-base-300/50 bg-base-100 shadow-panel-lg flex flex-col overflow-hidden rounded-2xl border";

export function PanelShell({
  depth,
  breadcrumbs,
  toolbar,
  footer,
  children,
  sidePanel,
}: PanelShellProps) {
  const mainPanel = (
    <div
      className={`${PANEL_CHROME} ${sidePanel ? "order-2 min-h-0 flex-1 lg:order-1 lg:basis-2/5" : "h-full"}`}
    >
      {/* Panel header */}
      <div className="border-base-300/50 bg-base-100/70 flex shrink-0 items-center gap-2.5 border-b px-4 py-2.75">
        {breadcrumbs}
        {toolbar && <div className="flex shrink-0 items-center gap-2">{toolbar}</div>}
      </div>

      {/* Panel body */}
      <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>

      {/* Panel footer */}
      <div className="border-base-300/50 bg-base-100/60 flex shrink-0 items-center gap-2 border-t px-4 py-2.25">
        <ShieldCheckIcon size={11} className="text-primary" />
        <span className="text-caption text-text-faint">{footer}</span>
        <div className="flex-1" />
        {depth > 1 && (
          <span className="font-display text-ui text-text-faint italic">{depth} panels deep</span>
        )}
      </div>
    </div>
  );

  return (
    <div className="relative flex-1 overflow-hidden p-5.5 pl-7">
      {/* Ghost panels — filing cabinet depth */}
      {depth >= 4 && (
        <div className="border-primary/10 bg-bg-ghost-1/60 animate-ghost-panel absolute inset-y-5.5 right-5.5 left-7 z-0 -translate-x-3.75 -translate-y-3.75 rounded-2xl border delay-150" />
      )}
      {depth >= 3 && (
        <div className="border-primary/15 bg-bg-ghost-1 animate-ghost-panel absolute inset-y-5.5 right-5.5 left-7 z-1 -translate-x-2.5 -translate-y-2.5 rounded-2xl border delay-75" />
      )}
      {depth >= 2 && (
        <div className="border-base-300/50 bg-bg-ghost-2 shadow-panel-sm animate-ghost-panel absolute inset-y-5.5 right-5.5 left-7 z-2 -translate-x-1.25 -translate-y-1.25 rounded-2xl border" />
      )}

      {sidePanel ? (
        /* Two-panel layout: main + side, stacked on mobile, side-by-side on desktop */
        <div className="absolute inset-y-5.5 right-5.5 left-7 z-10 flex flex-col gap-3 lg:flex-row">
          {mainPanel}
          {/* Side panel */}
          <div className={`${PANEL_CHROME} order-1 min-h-0 flex-1 lg:order-2`}>{sidePanel}</div>
        </div>
      ) : (
        /* Single-panel layout */
        <div className="absolute inset-y-5.5 right-5.5 left-7 z-10">{mainPanel}</div>
      )}
    </div>
  );
}
