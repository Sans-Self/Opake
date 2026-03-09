import { useMemo } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { ComponentPropsWithoutRef } from "react";

interface MarkdownPreviewProps {
  readonly data: Uint8Array;
}

export function MarkdownPreview({ data }: MarkdownPreviewProps) {
  const content = useMemo(() => new TextDecoder().decode(data), [data]);

  return (
    <div className="h-full overflow-y-auto p-6">
      <article className="prose prose-sm max-w-none">
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

  return (
    <SyntaxHighlighter
      style={oneDark}
      language={match[1]}
      PreTag="pre"
      customStyle={{ borderRadius: "0.5rem", fontSize: "0.8125rem" }}
    >
      {codeString}
    </SyntaxHighlighter>
  );
}
