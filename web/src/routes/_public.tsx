import { createFileRoute, Link, Outlet } from "@tanstack/react-router";
import { ArrowRightIcon } from "@phosphor-icons/react";
import { OpakeLogo } from "@/components/OpakeLogo";

interface NavItem {
  readonly label: string;
  readonly href: string;
  readonly internal?: boolean;
}

const NAV_LINKS: readonly NavItem[] = [
  { label: "FAQ", href: "/faq", internal: true },
  { label: "About", href: "/#what-is-opake", internal: true },
  { label: "How it works", href: "/#how-it-works", internal: true },
  { label: "Handbook", href: "/docs/", internal: true },
  { label: "Source code", href: "https://tangled.org/sans-self.org/opake.app" },
];

interface FooterLink {
  readonly label: string;
  readonly href: string;
  readonly internal?: boolean;
}

interface FooterGroup {
  readonly heading: string;
  readonly links: readonly FooterLink[];
}

const FOOTER_GROUPS: readonly FooterGroup[] = [
  {
    heading: "Product",
    links: [
      { label: "Open your cabinet", href: "/devices/login", internal: true },
      { label: "Handbook", href: "/docs/", internal: true },
      { label: "FAQ", href: "/faq", internal: true },
    ],
  },
  {
    heading: "Community",
    links: [{ label: "AT Protocol", href: "https://atproto.com" }],
  },
  {
    heading: "Resources",
    links: [
      { label: "Source Code", href: "https://tangled.org/sans-self.org/opake.app" },
      { label: "Report Issues", href: "https://tangled.org/sans-self.org/opake.app/issues" },
      {
        label: "Contributing",
        href: "https://tangled.org/sans-self.org/opake.app/tree/main/CONTRIBUTING.md",
      },
    ],
  },
];

function NavLink({
  label,
  href,
  internal,
}: {
  readonly label: string;
  readonly href: string;
  readonly internal?: boolean;
}) {
  const className = "text-text-muted hover:text-secondary transition-colors";

  if (internal) {
    return (
      <Link to={href} className={className}>
        {label}
      </Link>
    );
  }

  if (href.startsWith("#")) {
    const scrollToSection = (event: React.MouseEvent) => {
      event.preventDefault();
      const target = document.querySelector(href);
      if (target) {
        const navHeight = 72;
        const top = target.getBoundingClientRect().top + window.scrollY - navHeight;
        window.scrollTo({ top, behavior: "smooth" });
      }
    };

    return (
      <a href={href} onClick={scrollToSection} className={className}>
        {label}
      </a>
    );
  }

  return (
    <a href={href} target="_blank" rel="noopener noreferrer" className={className}>
      {label}
    </a>
  );
}

function PublicLayout() {
  return (
    <div className="bg-base-300 flex min-h-screen flex-col font-sans">
      {/* Nav — transparent, no border, matching screenshot */}
      <nav className="border-border-accent/30 bg-base-300/80 fixed inset-x-0 top-0 z-50 flex items-center justify-between border-b px-8 py-4 backdrop-blur-[14px] sm:px-12">
        <Link to="/">
          <OpakeLogo />
        </Link>

        <div className="text-ui hidden items-center gap-7 md:flex">
          {NAV_LINKS.map((link) => (
            <NavLink
              key={link.label}
              label={link.label}
              href={link.href}
              internal={link.internal}
            />
          ))}
        </div>

        <Link to="/cabinet" className="btn btn-neutral btn-sm text-ui gap-2">
          Open your cabinet
          <ArrowRightIcon size={14} />
        </Link>
      </nav>

      {/* Page content */}
      <main className="flex-1">
        <Outlet />
      </main>

      {/* Footer */}
      <footer className="border-border-accent/30 border-t">
        <div className="mx-auto max-w-5xl px-8 pt-20 pb-10 sm:px-12">
          {/* Top row — logo + link columns */}
          <div className="grid grid-cols-2 gap-12 sm:grid-cols-4">
            {/* Brand column */}
            <div className="col-span-2 sm:col-span-1">
              <OpakeLogo />
              <p className="text-text-muted mt-4 max-w-50 text-[0.78rem] leading-[1.75]">
                Encrypted by design.
                <br />
                Built with the AT&nbsp;Protocol.
              </p>
              <p className="text-text-faint text-caption mt-3 tracking-[0.04em]">
                Amsterdam · The Open Web
              </p>
            </div>

            {/* Link columns */}
            {FOOTER_GROUPS.map((group) => (
              <div key={group.heading}>
                <h4 className="text-label text-text-faint mb-4 tracking-widest uppercase">
                  {group.heading}
                </h4>
                <ul className="flex flex-col gap-2.5">
                  {group.links.map((link) => (
                    <li key={link.label}>
                      {link.internal ? (
                        <Link
                          to={link.href}
                          className="text-text-muted hover:text-base-content text-[0.8rem] transition-colors"
                        >
                          {link.label}
                        </Link>
                      ) : (
                        <a
                          href={link.href}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-text-muted hover:text-base-content text-[0.8rem] transition-colors"
                        >
                          {link.label}
                        </a>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>

          {/* Bottom bar */}
          <div className="border-border-accent/30 mt-16 flex flex-wrap items-center justify-between gap-4 border-t pt-6">
            <p className="text-text-faint text-caption tracking-wider">
              MMXXVI · Opake · All rights reserved
            </p>
            <p className="font-display text-text-faint text-caption tracking-wide italic">
              Privacy without the bunker.
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}

export const Route = createFileRoute("/_public")({
  component: PublicLayout,
});
