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
    });

    mermaidInitialized = true;
  }
  return mermaid;
}

interface MermaidBlockProps {
  readonly code: string;
}

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

  return <div ref={containerRef} className="my-4 flex justify-center [&>svg]:max-w-full" />;
}
