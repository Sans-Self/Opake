import { Suspense, lazy, useMemo } from "react";
import { cn } from "@/lib/cn";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { ComponentPropsWithoutRef } from "react";

const MermaidBlock = lazy(() =>
  import("@/components/content/MermaidBlock").then((m) => ({ default: m.MermaidBlock })),
);

interface MarkdownPreviewProps {
  /** Raw encrypted blob bytes (decoded on render). */
  readonly data?: Uint8Array;
  /** Pre-decoded markdown string (skips TextDecoder). */
  readonly content?: string;
  /** Use editor-style headings (sans-serif) instead of display headings. */
  readonly editorStyle?: boolean;
}

/** Strip HTML comments so they don't render as visible text. */
const HTML_COMMENT_RE = /<!--[\s\S]*?-->/g;

export function MarkdownPreview({ data, content: contentProp, editorStyle }: MarkdownPreviewProps) {
  const content = useMemo(
    () =>
      (contentProp ?? (data ? new TextDecoder().decode(data) : "")).replace(HTML_COMMENT_RE, ""),
    [contentProp, data],
  );

  return (
    <div className="h-full overflow-y-auto p-6">
      <article className={cn("prose prose-sm max-w-none", editorStyle && "prose-editor")}>
        <Markdown remarkPlugins={[remarkGfm]} components={{ code: CodeBlock }}>
          {content}
        </Markdown>
      </article>
    </div>
  );
}

function CodeBlock({ className, children, ...props }: Readonly<ComponentPropsWithoutRef<"code">>) {
  const match = /language-(\w+)/.exec(className ?? "");
  const codeString = Array.isArray(children)
    ? children
        .map((c) => (typeof c === "string" ? c : ""))
        .join("")
        .replace(/\n$/, "")
    : (typeof children === "string" ? children : "").replace(/\n$/, "");

  if (!match) {
    return (
      <code className={className} {...props}>
        {children}
      </code>
    );
  }

  if (match[1] === "mermaid") {
    return (
      <Suspense fallback={<pre className="text-caption text-text-muted p-4">Loading diagram…</pre>}>
        <MermaidBlock code={codeString} />
      </Suspense>
    );
  }

  return (
    <SyntaxHighlighter
      style={oneLight}
      language={match[1]}
      PreTag="pre"
      customStyle={{ borderRadius: "0.5rem", fontSize: "0.8125rem" }}
    >
      {codeString}
    </SyntaxHighlighter>
  );
}
