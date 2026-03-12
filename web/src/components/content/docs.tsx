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
