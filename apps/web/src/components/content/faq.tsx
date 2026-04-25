import type { ReactNode } from "react";

interface FaqSectionProps {
  readonly children: ReactNode;
}

export function FaqSection({ children }: FaqSectionProps) {
  return <div className="mx-auto flex max-w-3xl flex-col gap-2">{children}</div>;
}

interface FaqItemProps {
  readonly question: string;
  readonly children: ReactNode;
}

export function FaqItem({ question, children }: FaqItemProps) {
  return (
    <details className="collapse-arrow border-border-accent/40 bg-base-100 collapse rounded-xl border">
      <summary className="collapse-title text-base-content min-h-0 px-6 py-4 text-[0.95rem] font-medium">
        {question}
      </summary>
      <div className="collapse-content prose text-secondary px-6 pb-4 text-[0.9rem] leading-relaxed">
        {children}
      </div>
    </details>
  );
}
