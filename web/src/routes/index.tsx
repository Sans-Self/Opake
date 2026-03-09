import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRightIcon } from "@phosphor-icons/react";
import { OpakeLogo } from "@/components/OpakeLogo";
import LandingContent from "@/content/landing.mdx";

function LandingPage() {
  return (
    <div className="bg-base-300 flex min-h-screen flex-col font-sans">
      {/* Nav */}
      <nav className="border-base-300/50 bg-base-300/85 fixed inset-x-0 top-0 z-50 flex items-center justify-between border-b px-10 py-3.5 backdrop-blur-[14px]">
        <OpakeLogo />
        <Link to="/cabinet" className="btn btn-neutral btn-sm text-ui gap-2">
          Open the Cabinet
          <ArrowRightIcon size={14} />
        </Link>
      </nav>

      {/* Hero */}
      <section className="flex min-h-screen flex-col items-center justify-center px-10 pt-30 pb-20">
        {/* Ornamental rule */}
        <div className="divider text-caption text-primary before:bg-border-accent after:bg-border-accent mb-8 w-80 self-center tracking-[0.18em] uppercase">
          Built on the AT Protocol
        </div>

        <h1 className="font-display text-base-content mb-7 max-w-205 text-center text-[clamp(3.4rem,7.5vw,6.2rem)] leading-[1.04] font-normal tracking-tight">
          Your data, <em className="text-primary">freely shared</em>,
          <br />
          privately kept.
        </h1>

        <div className="prose text-secondary mb-10 max-w-130 text-center text-[1.05rem] leading-[1.75]">
          <LandingContent />
        </div>

        <div className="flex items-center gap-3.5">
          <Link
            to="/cabinet"
            className="btn btn-neutral gap-2.5 shadow-[0_4px_20px_oklch(0.155_0.035_70/0.18)]"
          >
            Open the Cabinet
            <ArrowRightIcon size={15} />
          </Link>
          <a
            href="#about"
            className="btn btn-outline border-border-accent text-secondary hover:bg-accent"
          >
            Learn more
          </a>
        </div>
      </section>
    </div>
  );
}

const DESCRIPTION =
  "Encrypted personal cloud built on the AT Protocol. Your files — encrypted, owned, shared on your terms.";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Opake — Your data, freely shared, privately kept" },
      { name: "description", content: DESCRIPTION },
      { name: "og:title", content: "Opake — Your data, freely shared, privately kept" },
      { name: "og:description", content: DESCRIPTION },
      { name: "twitter:title", content: "Opake — Your data, freely shared, privately kept" },
      { name: "twitter:description", content: DESCRIPTION },
    ],
  }),
  component: LandingPage,
});
