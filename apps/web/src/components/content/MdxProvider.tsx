import type { ComponentType } from "react";
import { Link } from "@tanstack/react-router";
import * as contentComponents from "./index";

/**
 * All custom components available to MDX files.
 * Standard HTML overrides (a → Link) are mixed in.
 */
const components: Readonly<Record<string, ComponentType<never>>> = {
  ...contentComponents,
  a: (({ href, children, ...rest }: { href?: string; children?: React.ReactNode }) => {
    if (href?.startsWith("/")) {
      return (
        <Link to={href} {...rest}>
          {children}
        </Link>
      );
    }
    return (
      <a href={href} target="_blank" rel="noopener noreferrer" {...rest}>
        {children}
      </a>
    );
  }) as ComponentType<never>,
};

interface MdxContentProps {
  readonly Content: ComponentType<{ readonly components?: Record<string, ComponentType<never>> }>;
  readonly className?: string;
}

export function MdxContent({ Content, className }: MdxContentProps) {
  return (
    <div className={className}>
      <Content components={components} />
    </div>
  );
}
