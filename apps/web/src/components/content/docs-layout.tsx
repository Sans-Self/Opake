import { useEffect, useState, type ReactNode } from "react";
import { CaretDownIcon } from "@phosphor-icons/react";
import { DocsSidebar } from "./docs-sidebar";

interface DocsLayoutProps {
  /** Slug of the current page, for the sidebar's active highlight. */
  readonly currentSlug?: string;
  /** Group of the current page. See {@link DocsSidebar} on why both are needed. */
  readonly currentGroup?: string;
  readonly children: ReactNode;
}

/**
 * Page shell for every public docs route: the article, plus documentation
 * navigation that survives narrow viewports. Below `lg` there is no room for
 * a persistent column, so the same tree renders inside a disclosure above the
 * article rather than disappearing.
 */
export function DocsLayout({ currentSlug, currentGroup, children }: DocsLayoutProps) {
  return (
    <div className="mx-auto w-full max-w-6xl px-6 pt-28 pb-20 sm:px-10">
      <DocsNavDisclosure currentSlug={currentSlug} currentGroup={currentGroup} />
      <div className="flex gap-10">
        <aside className="hidden shrink-0 lg:block lg:w-60">
          <div className="sticky top-24 max-h-[calc(100vh-7rem)] overflow-y-auto pr-1">
            <DocsSidebar currentSlug={currentSlug} currentGroup={currentGroup} />
          </div>
        </aside>
        {/* Not a <main>: the public route shell already owns that landmark. */}
        <div className="min-w-0 flex-1">{children}</div>
      </div>
    </div>
  );
}

const PANEL_ID = "docs-nav-panel";

function DocsNavDisclosure({
  currentSlug,
  currentGroup,
}: Omit<DocsLayoutProps, "children">) {
  const [open, setOpen] = useState(false);

  // Collapse on navigation. The panel pushes the article down, so leaving it
  // open would bury the page the reader just picked.
  useEffect(() => {
    setOpen(false);
  }, [currentSlug, currentGroup]);

  return (
    <div className="mb-8 lg:hidden">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={PANEL_ID}
        onClick={() => setOpen((isOpen) => !isOpen)}
        className="border-border-accent/40 bg-base-200/60 text-base-content text-ui hover:bg-accent/30 flex w-full items-center justify-between rounded-lg border px-4 py-2.5 font-medium transition-colors"
      >
        Browse the handbook
        <CaretDownIcon
          size={16}
          aria-hidden="true"
          className={`text-text-muted transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      <div
        id={PANEL_ID}
        hidden={!open}
        className="border-border-accent/30 mt-3 rounded-lg border px-4 py-4"
      >
        <DocsSidebar currentSlug={currentSlug} currentGroup={currentGroup} />
      </div>
    </div>
  );
}
