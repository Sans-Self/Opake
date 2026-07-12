import { DOCS_REGISTRY, docPath } from "./docs-registry";

/**
 * A single searchable hit. Today this is one per registered doc; future work
 * could extend it with per-heading hits once there's a build-time pre-pass
 * that extracts `##`+ headings from MDX source (the natural `import.meta.glob
 * + ?raw` approach collides with the MDX plugin's `enforce: "pre"` transform,
 * so section-anchor search needs its own small build step to land cleanly).
 */
export interface SearchHit {
  readonly docSlug: string;
  readonly docTitle: string;
  readonly docGroup: string | undefined;
  readonly docDescription: string;
  readonly href: string;
}

/**
 * All searchable hits. Memoised in module scope — the registry is a
 * build-time constant, so this array is stable for the lifetime of the app.
 */
const ALL_HITS: readonly SearchHit[] = DOCS_REGISTRY.map((doc) => ({
  docSlug: doc.slug,
  docTitle: doc.title,
  docGroup: doc.group,
  docDescription: doc.description,
  href: docPath(doc),
}));

/**
 * Simple substring match, case-insensitive, across title and description.
 * Results are ordered by match strength:
 *   1. title prefix match
 *   2. title contains
 *   3. description contains
 * Within each bucket, registry order is preserved.
 */
export function searchDocs(query: string): readonly SearchHit[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];

  const prefixMatches: readonly SearchHit[] = ALL_HITS.filter((hit) =>
    hit.docTitle.toLowerCase().startsWith(q),
  );
  const titleContainsMatches: readonly SearchHit[] = ALL_HITS.filter((hit) => {
    const title = hit.docTitle.toLowerCase();
    return !title.startsWith(q) && title.includes(q);
  });
  const descriptionMatches: readonly SearchHit[] = ALL_HITS.filter(
    (hit) => !hit.docTitle.toLowerCase().includes(q) && hit.docDescription.toLowerCase().includes(q),
  );

  return [...prefixMatches, ...titleContainsMatches, ...descriptionMatches];
}
