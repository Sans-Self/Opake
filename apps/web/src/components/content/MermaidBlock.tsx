import { useEffect, useRef, useState } from "react";

// eslint-disable-next-line functional/no-let -- module-level init guard for lazy-loaded mermaid
let mermaidInitialized = false;

async function getMermaid() {
  const { default: mermaid } = await import("mermaid");
  if (!mermaidInitialized) {
    mermaid.initialize({
      startOnLoad: false,
      theme: "neutral",
      fontFamily: "Inter, sans-serif",
      // Diagram source is decrypted user content — untrusted. Strict mode runs
      // mermaid's output through DOMPurify and disables click bindings and raw
      // HTML labels, so the SVG assigned via innerHTML below can't carry script.
      securityLevel: "strict",
    });

    mermaidInitialized = true;
  }
  return mermaid;
}

interface MermaidBlockProps {
  readonly code: string;
}

/**
 * Render a Mermaid diagram from its source string. Mermaid is dynamically
 * imported so the ~500 KB library only loads on pages that use it (cabinet
 * markdown preview, docs sequence diagrams).
 */
export function MermaidBlock({ code }: MermaidBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || !code.trim()) return;

    // eslint-disable-next-line functional/no-let -- cleanup flag for async effect
    let cancelled = false;

    const render = async () => {
      try {
        const mermaid = await getMermaid();
        const id = `mermaid-${crypto.randomUUID().slice(0, 8)}`;
        const { svg } = await mermaid.render(id, code.trim());
        if (!cancelled) {
          // eslint-disable-next-line functional/immutable-data -- mermaid returns SVG string, innerHTML is the intended API
          container.innerHTML = svg;
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Failed to render diagram");
        }
      }
    };

    void render();
    return () => {
      cancelled = true;
    };
  }, [code]);

  if (error) {
    return (
      <div className="border-error/20 bg-error/5 rounded-lg border p-3">
        <p className="text-caption text-error mb-1">Mermaid syntax error</p>
        <pre className="text-micro text-text-muted whitespace-pre-wrap">{error}</pre>
      </div>
    );
  }

  // `role="img"` marks the rendered SVG as a single image for assistive tech.
  // The accessible name comes from the wrapping `<figure aria-label=...>`
  // in SequenceDiagram; if MermaidBlock is used standalone (MarkdownPreview),
  // the surrounding context is expected to carry that meaning.
  return (
    <div ref={containerRef} role="img" className="my-4 flex justify-center [&>svg]:max-w-full" />
  );
}
