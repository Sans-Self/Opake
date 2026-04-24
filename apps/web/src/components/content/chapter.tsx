import {
  type ReactNode,
  type ReactElement,
  useState,
  Children,
  isValidElement,
  createContext,
  useContext,
} from "react";
import { InfoIcon, WarningIcon, DesktopIcon, TerminalIcon } from "@phosphor-icons/react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";

/* ─── Chapter header ───────────────────────────────────────────────────────── */

interface ChapterHeaderProps {
  readonly title: string;
}

export function ChapterHeader({ title }: ChapterHeaderProps) {
  return (
    <div className="mb-8">
      <h1 className="font-display text-base-content text-[clamp(1.8rem,4vw,2.8rem)] leading-[1.15] font-normal tracking-tight">
        {title}
      </h1>
    </div>
  );
}

/* ─── Lead paragraph ───────────────────────────────────────────────────────── */

interface ChildrenProps {
  readonly children: ReactNode;
}

export function Lead({ children }: ChildrenProps) {
  return <div className="text-secondary mb-8 text-[1.05rem] leading-[1.8]">{children}</div>;
}

/* ─── Callout ──────────────────────────────────────────────────────────────── */

const CALLOUT_STYLES = {
  info: {
    container: "border-info/30 bg-info/5",
    icon: InfoIcon,
    iconClass: "text-info",
  },
  warning: {
    container: "border-warning/30 bg-warning/5",
    icon: WarningIcon,
    iconClass: "text-warning",
  },
} as const;

interface CalloutProps {
  readonly type: "info" | "warning";
  readonly children: ReactNode;
}

export function Callout({ type, children }: CalloutProps) {
  const style = CALLOUT_STYLES[type];
  const IconComponent = style.icon;

  return (
    <aside
      role="note"
      className={`my-6 flex items-center gap-3 rounded-xl border p-4 ${style.container}`}
    >
      <IconComponent size={18} weight="fill" className={`mt-0.5 shrink-0 ${style.iconClass}`} />
      <div className="prose text-[0.88rem] leading-relaxed">{children}</div>
    </aside>
  );
}

/* ─── Platform toggle (Web App / CLI tabs) ─────────────────────────────────── */

const PlatformContext = createContext("Web App");

interface PlatformToggleProps {
  readonly children: ReactNode;
}

export function PlatformToggle({ children }: PlatformToggleProps) {
  const tabs = Children.toArray(children).filter(
    (child): child is ReactElement<PlatformTabProps> =>
      isValidElement(child) &&
      child.props != null &&
      typeof child.props === "object" &&
      "name" in child.props,
  );

  const tabNames = tabs.map((tab) => tab.props.name);
  const [active, setActive] = useState(tabNames[0] ?? "Web App");

  return (
    <div className="border-border-accent/40 my-6 overflow-hidden rounded-xl border">
      <div role="tablist" className="bg-base-200/60 flex border-b border-inherit">
        {tabNames.map((name) => {
          const isActive = name === active;
          const Icon = name === "CLI" ? TerminalIcon : DesktopIcon;

          return (
            <button
              key={name}
              role="tab"
              aria-selected={isActive}
              onClick={() => setActive(name)}
              className={`text-ui flex items-center gap-1.5 px-4 py-2.5 font-medium transition-colors ${
                isActive
                  ? "border-primary text-base-content border-b-2"
                  : "text-text-muted hover:text-secondary"
              }`}
            >
              <Icon size={14} />
              {name}
            </button>
          );
        })}
      </div>
      <div className="bg-base-100 p-4">
        <PlatformContext.Provider value={active}>{children}</PlatformContext.Provider>
      </div>
    </div>
  );
}

interface PlatformTabProps {
  readonly name: string;
  readonly children: ReactNode;
}

export function PlatformTab({ name, children }: PlatformTabProps) {
  const active = useContext(PlatformContext);
  if (name !== active) return null;

  return <div className="prose text-[0.88rem] leading-relaxed">{children}</div>;
}

/* ─── Code block ───────────────────────────────────────────────────────────── */

interface CodeBlockProps {
  readonly language: string;
  readonly title?: string;
  readonly children?: ReactNode;
  /**
   * Explicit code string. When passed, bypasses `children` extraction.
   * Use this for multi-line snippets in MDX: JSX children go through MDX's
   * whitespace normalization (which strips the minimum indent among
   * indented lines), but JSX attribute strings pass through untouched.
   */
  readonly code?: string;
}

function extractText(node: ReactNode): string {
  if (typeof node === "string") return node;
  if (typeof node === "number") return String(node);
  if (node == null || typeof node === "boolean") return "";
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (typeof node === "object" && "props" in node)
    return extractText((node.props as { children?: ReactNode }).children);
  return "";
}

export function CodeBlock({ language, title, children, code }: CodeBlockProps) {
  const source = code ?? extractText(children);
  const trimmed = source.trim();

  return (
    <div className="not-prose border-border-accent/30 my-4 overflow-hidden rounded-lg border">
      {title && (
        <div className="bg-base-200/60 border-b border-inherit px-4 py-1.5">
          <span className="text-text-muted font-mono text-[0.72rem]">{title}</span>
        </div>
      )}
      <SyntaxHighlighter
        language={language}
        style={oneLight}
        customStyle={{
          margin: 0,
          padding: "1rem",
          fontSize: "0.82rem",
          lineHeight: "1.55",
          whiteSpace: "pre",
        }}
        codeTagProps={{
          style: {
            fontFamily:
              'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
            whiteSpace: "pre",
          },
        }}
      >
        {trimmed}
      </SyntaxHighlighter>
    </div>
  );
}

/* ─── Sequence diagram (placeholder) ───────────────────────────────────────── */

interface SequenceDiagramProps {
  readonly id: string;
}

export function SequenceDiagram({ id }: SequenceDiagramProps) {
  return (
    <div className="border-border-accent/30 bg-base-200/30 my-6 flex min-h-32 items-center justify-center rounded-xl border border-dashed">
      <span className="text-text-muted text-ui italic">Diagram: {id}</span>
    </div>
  );
}
