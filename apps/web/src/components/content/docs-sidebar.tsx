import { Link } from "@tanstack/react-router";
import { CATEGORY_META, findDoc, partitionCategoryForSidebar, type DocMeta } from "@/lib/docs-registry";

interface DocsSidebarProps {
  /**
   * Slug of the current page. Together with `currentGroup`, used to
   * render the active highlight. Two docs can share a slug (`use/sharing`
   * vs `sdk/sharing`), so the group is required for disambiguation on
   * nested routes.
   */
  readonly currentSlug?: string;
  /**
   * Group of the current page (e.g. `"sdk"`). Pass `undefined` for flat
   * docs. Paired with `currentSlug` to identify the active item uniquely.
   */
  readonly currentGroup?: string;
  /**
   * Visual density variant. `public` renders at full width for the marketing
   * docs layout; `cabinet` is compact to fit inside the file-browser panel.
   */
  readonly variant?: "public" | "cabinet";
}

function isCurrent(
  doc: DocMeta,
  currentSlug: string | undefined,
  currentGroup: string | undefined,
): boolean {
  if (currentSlug === undefined) return false;
  if (doc.slug !== currentSlug) return false;
  return doc.group === currentGroup;
}

/**
 * Persistent docs table of contents. Reads directly from `DOCS_REGISTRY`,
 * so registering a new doc there is all that's needed for it to show up in
 * the sidebar. Rendered on every doc page (index, flat slug, nested slug).
 */
export function DocsSidebar({
  currentSlug,
  currentGroup,
  variant = "public",
}: DocsSidebarProps) {
  const faq = findDoc("faq");

  const baseSectionGap = variant === "cabinet" ? "space-y-4" : "space-y-5";
  const headingSize = variant === "cabinet" ? "text-caption" : "text-ui";
  const linkSize = variant === "cabinet" ? "text-caption" : "text-ui";

  return (
    <nav aria-label="Documentation" className={`${baseSectionGap} text-sm`}>
      <Link
        to="/docs"
        className={`text-text-muted hover:text-base-content ${linkSize} block font-medium`}
      >
        Docs home
      </Link>

      {CATEGORY_META.map((cat) => {
        const { ungrouped, groups } = partitionCategoryForSidebar(cat.key);
        if (ungrouped.length === 0 && groups.length === 0) return null;
        return (
          <section key={cat.key}>
            <div
              className={`text-text-muted mb-2 ${headingSize} font-medium tracking-wide uppercase`}
            >
              {cat.label}
            </div>
            <ul className="flex flex-col gap-0.5">
              {ungrouped.map((doc) => (
                <SidebarLink
                  key={doc.slug}
                  doc={doc}
                  isCurrent={isCurrent(doc, currentSlug, currentGroup)}
                  linkSize={linkSize}
                />
              ))}
              {groups.map((group) => (
                <li key={group.key} className="mt-2">
                  <div
                    className={`text-text-muted ${headingSize} mb-1 pl-1 font-medium opacity-80`}
                  >
                    {group.label}
                  </div>
                  <ul className="border-border-accent/30 ml-1.5 flex flex-col gap-0.5 border-l pl-2">
                    {group.docs.map((doc) => (
                      <SidebarLink
                        key={doc.slug}
                        doc={doc}
                        isCurrent={isCurrent(doc, currentSlug, currentGroup)}
                        linkSize={linkSize}
                      />
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        );
      })}

      {faq && (
        <section>
          <ul className="flex flex-col gap-0.5">
            <SidebarLink
              doc={faq}
              isCurrent={isCurrent(faq, currentSlug, currentGroup)}
              linkSize={linkSize}
            />
          </ul>
        </section>
      )}
    </nav>
  );
}

interface SidebarLinkProps {
  readonly doc: DocMeta;
  readonly isCurrent: boolean;
  readonly linkSize: string;
}

function SidebarLink({ doc, isCurrent, linkSize }: SidebarLinkProps) {
  const className = isCurrent
    ? `text-primary ${linkSize} block rounded px-2 py-1 font-medium`
    : `text-text-muted hover:text-base-content hover:bg-accent/30 ${linkSize} block rounded px-2 py-1 transition-colors`;
  const ariaCurrent = isCurrent ? "page" : undefined;

  // TanStack's typed Link forks on nested vs flat just like the docs index
  // card — the `to` argument's shape has to match the registered route.
  return (
    <li>
      {doc.group ? (
        <Link
          to="/docs/$category/$slug"
          params={{ category: doc.group, slug: doc.slug }}
          className={className}
          aria-current={ariaCurrent}
        >
          {doc.title}
        </Link>
      ) : (
        <Link
          to="/docs/$slug"
          params={{ slug: doc.slug }}
          className={className}
          aria-current={ariaCurrent}
        >
          {doc.title}
        </Link>
      )}
    </li>
  );
}

/**
 * Variant that renders `Link`s pointing at the cabinet docs routes instead
 * of the public ones. Same shape, different `to` targets.
 */
export function DocsSidebarCabinet({
  currentSlug,
  currentGroup,
}: {
  readonly currentSlug?: string;
  readonly currentGroup?: string;
}) {
  const faq = findDoc("faq");

  return (
    <nav aria-label="Documentation" className="text-caption space-y-4">
      <Link
        to="/cabinet/docs"
        className="text-text-muted hover:text-base-content block text-xs font-medium"
      >
        Docs home
      </Link>

      {CATEGORY_META.map((cat) => {
        const { ungrouped, groups } = partitionCategoryForSidebar(cat.key);
        if (ungrouped.length === 0 && groups.length === 0) return null;
        return (
          <section key={cat.key}>
            <div className="text-text-muted mb-1.5 text-[0.65rem] font-medium tracking-wide uppercase">
              {cat.label}
            </div>
            <ul className="flex flex-col gap-0.5">
              {ungrouped.map((doc) => (
                <CabinetLink
                  key={doc.slug}
                  doc={doc}
                  isCurrent={isCurrent(doc, currentSlug, currentGroup)}
                />
              ))}
              {groups.map((group) => (
                <li key={group.key} className="mt-1.5">
                  <div className="text-text-muted mb-1 pl-1 text-[0.65rem] font-medium opacity-80">
                    {group.label}
                  </div>
                  <ul className="border-border-accent/30 ml-1 flex flex-col gap-0.5 border-l pl-1.5">
                    {group.docs.map((doc) => (
                      <CabinetLink
                        key={doc.slug}
                        doc={doc}
                        isCurrent={isCurrent(doc, currentSlug, currentGroup)}
                      />
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        );
      })}

      {faq && (
        <section>
          <ul>
            <CabinetLink doc={faq} isCurrent={isCurrent(faq, currentSlug, currentGroup)} />
          </ul>
        </section>
      )}
    </nav>
  );
}

function CabinetLink({ doc, isCurrent }: { readonly doc: DocMeta; readonly isCurrent: boolean }) {
  const className = isCurrent
    ? "text-primary block rounded px-1.5 py-0.5 text-xs font-medium"
    : "text-text-muted hover:text-base-content hover:bg-accent/30 block rounded px-1.5 py-0.5 text-xs transition-colors";
  const ariaCurrent = isCurrent ? "page" : undefined;

  return (
    <li>
      {doc.group ? (
        <Link
          to="/cabinet/docs/$category/$slug"
          params={{ category: doc.group, slug: doc.slug }}
          className={className}
          aria-current={ariaCurrent}
        >
          {doc.title}
        </Link>
      ) : (
        <Link
          to="/cabinet/docs/$slug"
          params={{ slug: doc.slug }}
          className={className}
          aria-current={ariaCurrent}
        >
          {doc.title}
        </Link>
      )}
    </li>
  );
}
