import { createElement, type ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { resolveIcon } from "./icons";

/* ─── Hero ─────────────────────────────────────────────────────────────────── */

interface ChildrenProps {
  readonly children: ReactNode;
}

export function HeroSection({ children }: ChildrenProps) {
  return (
    <section className="relative flex min-h-screen flex-col items-center justify-center overflow-hidden px-6 pt-28 pb-20 sm:px-10">
      {/* Warm radial wash */}
      <div
        className="pointer-events-none absolute inset-0"
        style={{
          background:
            "radial-gradient(ellipse 70% 55% at 50% -5%, rgba(160, 125, 60, 0.09) 0%, transparent 65%)",
        }}
      />
      <div className="relative z-10 flex flex-col items-center">{children}</div>
    </section>
  );
}

export function HeroHeadline({ children }: ChildrenProps) {
  return (
    <h1 className="font-display text-base-content mb-7 max-w-208 text-center text-[clamp(2.6rem,7.5vw,6.2rem)] leading-[1.04] font-normal tracking-tight">
      {children}
    </h1>
  );
}

export function Highlight({ children }: ChildrenProps) {
  return <em className="text-primary not-italic">{children}</em>;
}

export function HeroSubtext({ children }: ChildrenProps) {
  return (
    <div className="text-secondary mx-auto mb-10 max-w-lg text-center text-[1.05rem] leading-[1.75]">
      {children}
    </div>
  );
}

/* ─── CTAs ─────────────────────────────────────────────────────────────────── */

export function CtaGroup({ children }: ChildrenProps) {
  return <div className="flex items-center gap-3.5">{children}</div>;
}

interface CtaProps {
  readonly href: string;
  readonly children: ReactNode;
}

export function PrimaryCta({ href, children }: CtaProps) {
  return (
    <Link
      to={href}
      className="btn btn-neutral gap-2.5 shadow-[0_4px_20px_oklch(0.155_0.035_70/0.18)]"
    >
      {children}
    </Link>
  );
}

export function SecondaryCta({ href, children }: CtaProps) {
  return (
    <Link to={href} className="btn btn-outline border-border-accent text-secondary hover:bg-accent">
      {children}
    </Link>
  );
}

/* ─── Divider ──────────────────────────────────────────────────────────────── */

interface DividerProps {
  readonly text: string;
}

export function Divider({ text }: DividerProps) {
  return (
    <div
      id={text
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        // eslint-disable-next-line sonarjs/slow-regex -- non-user input
        .replace(/-+$/, "")}
      className="divider text-caption text-primary before:bg-border-accent after:bg-border-accent mx-auto mb-8 w-80 tracking-[0.18em] uppercase"
    >
      {text}
    </div>
  );
}

/* ─── Section ──────────────────────────────────────────────────────────────── */

interface SectionProps {
  readonly id?: string;
  readonly children: ReactNode;
  readonly surface?: "default" | "raised";
}

export function Section({ id, children, surface = "default" }: SectionProps) {
  const bg = surface === "raised" ? "bg-base-100" : "";
  return (
    <section id={id} className={`border-border-accent/30 border-t px-6 py-32 sm:px-10 ${bg}`}>
      <div className="mx-auto max-w-5xl">{children}</div>
    </section>
  );
}

export function SectionHeader({ children }: ChildrenProps) {
  return (
    <h2 className="font-display text-base-content mx-auto mb-12 max-w-3xl text-center text-[clamp(1.8rem,4vw,3rem)] leading-[1.15] font-normal tracking-tight">
      {children}
    </h2>
  );
}

/* ─── Info grid (2-col layout: prose left, cards right) ────────────────────── */

interface InfoGridProps {
  readonly description: ReactNode;
  readonly children: ReactNode;
}

export function InfoGrid({ description, children }: InfoGridProps) {
  return (
    <div className="grid grid-cols-1 items-center gap-20 lg:grid-cols-2">
      <div>{description}</div>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">{children}</div>
    </div>
  );
}

interface InfoCardProps {
  readonly icon: string;
  readonly title: string;
  readonly children: ReactNode;
}

export function InfoCard({ icon, title, children }: InfoCardProps) {
  return (
    <div className="card border-border-accent/40 bg-base-100 shadow-panel-sm rounded-xl border p-5">
      <div className="bg-accent mb-3 flex size-8.5 items-center justify-center rounded-[9px]">
        {createElement(resolveIcon(icon), { size: 16, className: "text-primary" })}
      </div>
      <h3 className="text-base-content mb-1.5 text-[0.85rem] font-medium">{title}</h3>
      <p className="text-text-muted text-[0.75rem] leading-[1.6]">{children}</p>
    </div>
  );
}

/* ─── Step cards (numbered row) ────────────────────────────────────────────── */

export function StepGrid({ children }: ChildrenProps) {
  return <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">{children}</div>;
}

interface StepCardProps {
  readonly num: string;
  readonly icon: string;
  readonly featured?: boolean;
  readonly children: ReactNode;
}

export function StepCard({ num, icon, featured, children }: StepCardProps) {
  return (
    <div className="card border-border-accent/40 bg-base-300 rounded-xl border p-6">
      <div className="mb-4 flex items-center justify-between">
        <span className="font-display text-text-faint text-sm tracking-wide uppercase italic">
          {num}
        </span>
        <div
          className={`flex size-10 items-center justify-center rounded-xl ${featured ? "bg-base-content" : "bg-accent"}`}
        >
          {createElement(resolveIcon(icon), {
            size: 18,
            className: featured ? "text-base-100" : "text-primary",
          })}
        </div>
      </div>
      {children}
    </div>
  );
}

export function StepTitle({ children }: ChildrenProps) {
  return <h3 className="text-base-content mb-1.5 text-[0.9rem] font-medium">{children}</h3>;
}

export function StepBody({ children }: ChildrenProps) {
  return <p className="text-text-muted text-ui leading-relaxed">{children}</p>;
}

/* ─── CTA block ────────────────────────────────────────────────────────────── */

interface CenterActionProps {
  readonly headline: ReactNode;
  readonly subtext: ReactNode;
  readonly children: ReactNode;
}

export function CenterAction({ headline, subtext, children }: CenterActionProps) {
  return (
    <section className="px-6 py-32 sm:px-10">
      <div className="bg-neutral text-neutral-content relative mx-auto max-w-195 overflow-hidden rounded-2xl px-10 py-18 text-center sm:px-16">
        {/* Warm inner glow */}
        <div
          className="pointer-events-none absolute inset-0"
          style={{
            background:
              "radial-gradient(ellipse at 30% 0%, rgba(154, 120, 64, 0.18) 0%, transparent 55%)",
          }}
        />
        {/* Ornament */}
        <div className="font-display text-neutral-content relative z-10 mb-6 text-[28px] tracking-wide">
          — ☙ ⚷ ❧ —
        </div>
        <div className="relative z-10 flex flex-col items-center">
          <h2 className="font-display mb-4 text-[clamp(2rem,4vw,2.9rem)] leading-[1.18] font-normal">
            {headline}
          </h2>
          <p className="text-neutral-content/80 mx-auto mb-10 max-w-sm text-base leading-[1.75]">
            {subtext}
          </p>
          {children}
        </div>
      </div>
    </section>
  );
}

interface TextLinkProps {
  readonly href: string;
  readonly children: ReactNode;
}

export function TextLink({ href, children }: TextLinkProps) {
  const isExternal = href.startsWith("http");
  const className =
    "inline-flex items-center gap-1.5 text-[0.95rem] font-medium text-primary hover:text-primary/80";

  if (isExternal) {
    return (
      <a href={href} target="_blank" rel="noopener noreferrer" className={className}>
        {children}
      </a>
    );
  }

  return (
    <Link to={href} className={className}>
      {children}
    </Link>
  );
}

/* Re-export ArrowRightIcon so MDX can reference it without an import */
export { ArrowRightIcon } from "@phosphor-icons/react";
