import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRight } from "@phosphor-icons/react";
import { OpakeLogo } from "@/components/OpakeLogo";

function LandingPage() {
  return (
    <div className="flex min-h-screen flex-col bg-base-300 font-sans">
      {/* Nav */}
      <nav className="fixed inset-x-0 top-0 z-50 flex items-center justify-between border-b border-base-300/50 bg-base-300/85 px-10 py-3.5 backdrop-blur-[14px]">
        <OpakeLogo />
        <Link
          to="/cabinet"
          className="btn btn-neutral btn-sm gap-2 text-ui"
        >
          Open the Cabinet
          <ArrowRight size={14} />
        </Link>
      </nav>

      {/* Hero */}
      <section className="flex min-h-screen flex-col items-center justify-center px-10 pt-30 pb-20">
        {/* Ornamental rule */}
        <div className="divider mb-8 w-80 self-center text-caption uppercase tracking-[0.18em] text-primary before:bg-border-accent after:bg-border-accent">
          Built on the AT Protocol
        </div>

        <h1 className="mb-7 max-w-205 text-center font-display text-[clamp(3.4rem,7.5vw,6.2rem)] leading-[1.04] tracking-tight font-normal text-base-content">
          Your data,{" "}
          <em className="text-primary">freely shared</em>,
          <br />
          privately kept.
        </h1>

        <p className="mb-10 max-w-130 text-center text-[1.05rem] leading-[1.75] text-secondary">
          Opake exists because privacy and collaboration should not be a
          tradeoff. Your files — encrypted, owned, shared on your terms — through
          decentralised identity, with no central authority in between.
        </p>

        <div className="flex items-center gap-3.5">
          <Link
            to="/cabinet"
            className="btn btn-neutral gap-2.5 shadow-[0_4px_20px_oklch(0.155_0.035_70/0.18)]"
          >
            Open the Cabinet
            <ArrowRight size={15} />
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

export const Route = createFileRoute("/")({
  component: LandingPage,
});
