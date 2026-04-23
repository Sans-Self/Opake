import { createElement, type ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { ArrowRightIcon } from "@phosphor-icons/react";
import { resolveIcon } from "./icons";

/* ─── Docs index header ────────────────────────────────────────────────────── */

interface ChildrenProps {
  readonly children: ReactNode;
}

export function DocsHeader({ children }: ChildrenProps) {
  return <div className="mx-auto mb-12 max-w-3xl text-center">{children}</div>;
}

export function DocsTitle({ children }: ChildrenProps) {
  return (
    <h1 className="font-display text-base-content mb-3 text-[clamp(2rem,5vw,3.4rem)] leading-[1.1] font-normal tracking-tight">
      {children}
    </h1>
  );
}

export function DocsSubtitle({ children }: ChildrenProps) {
  return <p className="text-secondary text-[1.05rem] leading-relaxed">{children}</p>;
}

/* ─── Docs index grid ──────────────────────────────────────────────────────── */

export function DocsIndexGrid({ children }: ChildrenProps) {
  return (
    <div className="mx-auto grid max-w-4xl grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {children}
    </div>
  );
}

interface DocsIndexCardProps {
  readonly href: string;
  readonly icon: string;
  readonly children: ReactNode;
}

export function DocsIndexCard({ href, icon, children }: DocsIndexCardProps) {
  return (
    <Link
      to={href}
      className="card border-border-accent/40 bg-base-100 group hover:shadow-panel-sm rounded-xl border p-5 transition-shadow"
    >
      <div className="mb-3 flex items-center justify-between">
        <div className="bg-accent flex size-8 items-center justify-center rounded-lg">
          {createElement(resolveIcon(icon), { size: 16, className: "text-primary" })}
        </div>
        <ArrowRightIcon
          size={14}
          className="text-text-faint transition-transform group-hover:translate-x-0.5"
        />
      </div>
      {children}
    </Link>
  );
}

export function DocsIndexTitle({ children }: ChildrenProps) {
  return <h3 className="text-base-content mb-1 text-[0.9rem] font-medium">{children}</h3>;
}

export function DocsIndexBody({ children }: ChildrenProps) {
  return <p className="text-text-muted text-ui leading-relaxed">{children}</p>;
}

/* ─── Primary CTA — "Just getting started?" ──────────────────────────────── */

interface DocsIndexPrimaryProps {
  readonly href: string;
  readonly icon: string;
  readonly title: string;
  readonly body: string;
}

/**
 * Big, opinionated card at the top of the docs landing. Catches visitors
 * who don't yet know which audience they belong to — clicking drops them
 * into "Getting Started" directly, no classification required.
 */
export function DocsIndexPrimary({ href, icon, title, body }: DocsIndexPrimaryProps) {
  return (
    <div className="mx-auto mb-10 max-w-4xl">
      <Link
        to={href}
        className="border-primary/40 bg-base-100 group hover:border-primary/70 hover:shadow-panel flex items-center gap-5 rounded-2xl border-2 p-6 transition-all"
      >
        <div className="bg-primary/15 flex size-14 shrink-0 items-center justify-center rounded-xl">
          {createElement(resolveIcon(icon), { size: 26, className: "text-primary" })}
        </div>
        <div className="flex-1">
          <h2 className="text-base-content mb-1 text-lg font-medium">{title}</h2>
          <p className="text-text-muted leading-relaxed">{body}</p>
        </div>
        <ArrowRightIcon
          size={20}
          className="text-primary transition-transform group-hover:translate-x-1"
        />
      </Link>
    </div>
  );
}

/* ─── Secondary CTA — quiet inline link for confused visitors ───────────── */

interface DocsIndexSecondaryProps {
  readonly href: string;
  readonly icon: string;
  readonly label: string;
  readonly body: string;
}

/**
 * Slim companion to {@link DocsIndexPrimary}. Use for entry points that
 * aren't the main "try Opake" action but still catch early confusion — the
 * FAQ being the obvious one. Rendered as a muted bar under the primary CTA
 * so it doesn't compete for the user's first click.
 */
export function DocsIndexSecondary({ href, icon, label, body }: DocsIndexSecondaryProps) {
  return (
    <div className="mx-auto mb-10 max-w-4xl">
      <Link
        to={href}
        className="border-border-accent/40 bg-base-100/60 group hover:border-primary/60 hover:bg-base-100 flex items-center gap-3 rounded-xl border p-3.5 transition-all"
      >
        <div className="bg-accent/60 flex size-8 shrink-0 items-center justify-center rounded-lg">
          {createElement(resolveIcon(icon), { size: 15, className: "text-primary" })}
        </div>
        <div className="flex-1">
          <span className="text-base-content text-ui font-medium">{label}</span>
          <span className="text-text-muted text-ui ml-1.5">{body}</span>
        </div>
        <ArrowRightIcon
          size={14}
          className="text-text-muted group-hover:text-primary transition-colors"
        />
      </Link>
    </div>
  );
}

/* ─── Section wrapper — "For users", "Under the hood", etc. ─────────────── */

interface DocsIndexSectionProps {
  readonly label: string;
  readonly description?: string;
  readonly children: ReactNode;
}

export function DocsIndexSection({ label, description, children }: DocsIndexSectionProps) {
  return (
    <section className="mx-auto mt-10 max-w-4xl">
      <div className="mb-4">
        <h2 className="text-base-content text-[1.1rem] font-medium">{label}</h2>
        {description && (
          <p className="text-text-muted text-ui mt-0.5 leading-relaxed">{description}</p>
        )}
      </div>
      {children}
    </section>
  );
}
