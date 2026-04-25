import {
  type ReactNode,
  type ReactElement,
  type KeyboardEvent,
  Suspense,
  lazy,
  useId,
  useRef,
  useState,
  Children,
  isValidElement,
} from "react";
import { InfoIcon, WarningIcon, DesktopIcon, TerminalIcon } from "@phosphor-icons/react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";

/* Mermaid is ~500 KB and only loads when a page actually renders a diagram. */
const MermaidBlock = lazy(() =>
  import("./MermaidBlock").then((m) => ({ default: m.MermaidBlock })),
);

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
    /** Visually-hidden prefix so screen readers announce context ("Note:" vs "Warning:"). */
    srLabel: "Note:",
  },
  warning: {
    container: "border-warning/30 bg-warning/5",
    icon: WarningIcon,
    iconClass: "text-warning",
    srLabel: "Warning:",
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
      <IconComponent
        size={18}
        weight="fill"
        aria-hidden="true"
        className={`mt-0.5 shrink-0 ${style.iconClass}`}
      />
      <div className="prose text-[0.88rem] leading-relaxed">
        <span className="sr-only">{style.srLabel} </span>
        {children}
      </div>
    </aside>
  );
}

/* ─── Platform toggle (Web App / CLI tabs) ─────────────────────────────────── */

interface PlatformToggleProps {
  readonly children: ReactNode;
}

/**
 * WAI-ARIA tabs pattern for "how do I do this on the web app / on the CLI?"
 * blocks. Implements the manual-activation variant of the
 * [Tabs APG pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/):
 * left/right/home/end navigate between tabs and also activate them, since
 * each tab swap is cheap (just toggles a `hidden` attribute).
 *
 * PlatformTab is a prop-bag component whose props (`name`, `children`) are
 * read here — it doesn't render anything on its own. All panels render into
 * the DOM and `hidden` is used to mask inactive ones, which keeps
 * `aria-controls` pointing at a real tabpanel element whether or not it's
 * currently visible.
 */
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
  const id = useId();
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const moveFocus = (currentIndex: number, key: string): boolean => {
    // eslint-disable-next-line functional/no-let -- computed branching below
    let nextIndex = -1;
    if (key === "ArrowRight") nextIndex = (currentIndex + 1) % tabNames.length;
    else if (key === "ArrowLeft")
      nextIndex = (currentIndex - 1 + tabNames.length) % tabNames.length;
    else if (key === "Home") nextIndex = 0;
    else if (key === "End") nextIndex = tabNames.length - 1;

    if (nextIndex < 0) return false;

    const nextName = tabNames[nextIndex];
    if (nextName) {
      setActive(nextName);
      tabRefs.current[nextIndex]?.focus();
    }
    return true;
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (moveFocus(index, event.key)) {
      event.preventDefault();
    }
  };

  return (
    <div className="border-border-accent/40 my-6 overflow-hidden rounded-xl border">
      <div
        role="tablist"
        aria-label="Platform"
        className="bg-base-200/60 flex border-b border-inherit"
      >
        {tabNames.map((name, index) => {
          const isActive = name === active;
          const Icon = name === "CLI" ? TerminalIcon : DesktopIcon;

          return (
            <button
              key={name}
              type="button"
              role="tab"
              id={`${id}-tab-${index}`}
              aria-controls={`${id}-panel-${index}`}
              aria-selected={isActive}
              tabIndex={isActive ? 0 : -1}
              ref={(el) => {
                // eslint-disable-next-line functional/immutable-data -- ref array mutation is the React pattern
                tabRefs.current[index] = el;
              }}
              onClick={() => setActive(name)}
              onKeyDown={(event) => handleKeyDown(event, index)}
              className={`text-ui flex items-center gap-1.5 px-4 py-2.5 font-medium transition-colors ${
                isActive
                  ? "border-primary text-base-content border-b-2"
                  : "text-text-muted hover:text-secondary"
              }`}
            >
              <Icon size={14} aria-hidden="true" />
              {name}
            </button>
          );
        })}
      </div>
      {tabs.map((tab, index) => {
        const isActive = tab.props.name === active;
        return (
          <div
            key={tab.props.name}
            role="tabpanel"
            id={`${id}-panel-${index}`}
            aria-labelledby={`${id}-tab-${index}`}
            hidden={!isActive}
            tabIndex={0}
            className="bg-base-100 p-4"
          >
            <div className="prose text-[0.88rem] leading-relaxed">{tab.props.children}</div>
          </div>
        );
      })}
    </div>
  );
}

interface PlatformTabProps {
  readonly name: string;
  readonly children: ReactNode;
}

/**
 * Declarative marker for a single panel inside a {@link PlatformToggle}.
 * The parent `PlatformToggle` reads this element's `name` and `children`
 * props directly; this function body is only used as a fallback when the
 * component is rendered outside a toggle (e.g. a misnested page).
 */
export function PlatformTab({ children }: PlatformTabProps) {
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
  /** Mermaid source. Typically imported from the page's local `_diagrams.ts`. */
  readonly code: string;
  /** Short caption rendered below the diagram for context and accessibility. */
  readonly caption?: string;
}

/**
 * Render a Mermaid sequence diagram (or any Mermaid-supported flowchart) in a
 * docs page. Use alongside short prose explaining what the diagram shows —
 * the caption is the primary accessible label, since the generated SVG isn't
 * structured for screen readers.
 */
export function SequenceDiagram({ code, caption }: SequenceDiagramProps) {
  return (
    <figure className="not-prose my-8" role="group" aria-label={caption}>
      <div className="border-border-accent/30 bg-base-200/30 overflow-x-auto rounded-xl border p-4">
        <Suspense
          fallback={
            <div className="text-text-muted text-ui flex min-h-32 items-center justify-center italic">
              Loading diagram…
            </div>
          }
        >
          <MermaidBlock code={code} />
        </Suspense>
      </div>
      {caption && (
        <figcaption className="text-text-muted text-ui mt-3 text-center italic">
          {caption}
        </figcaption>
      )}
    </figure>
  );
}
