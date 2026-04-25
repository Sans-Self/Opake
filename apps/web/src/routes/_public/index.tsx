import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRightIcon } from "@phosphor-icons/react";
import { ogMeta } from "@/lib/og-meta";
import {
  HeroSection,
  HeroHeadline,
  Highlight,
  HeroSubtext,
  CtaGroup,
  PrimaryCta,
  SecondaryCta,
  Divider,
  SectionHeader,
  InfoGrid,
  InfoCard,
  StepGrid,
  StepCard,
  StepTitle,
  StepBody,
  CenterAction,
  TextLink,
  Section,
} from "@/components/content/landing";
import { CabinetMockup } from "@/components/CabinetMockup";

const DESCRIPTION =
  "Encrypted personal cloud built on the AT Protocol. Your files — encrypted, owned, shared on your terms.";

function LandingPage() {
  return (
    <>
      <HeroSection>
        <HeroHeadline>
          Your data; <Highlight>freely shared</Highlight>, privately kept.
        </HeroHeadline>

        <HeroSubtext>
          Private collaboration, finally made simple. Your files belong to you, they&apos;re shared
          on your terms, and no one else has the keys.
        </HeroSubtext>

        <CtaGroup>
          <PrimaryCta href="/cabinet">
            Open your cabinet <ArrowRightIcon size="1em" />
          </PrimaryCta>
          <SecondaryCta href="/docs">Learn more</SecondaryCta>
        </CtaGroup>

        <div className="mt-18 w-full max-w-3xl">
          <CabinetMockup />
        </div>
      </HeroSection>

      <Section>
        <Divider text="What is Opake?" />
        <InfoGrid
          description={
            <>
              <SectionHeader>
                An encrypted cloud that answers to <Highlight>you</Highlight>, not a big tech
                company.
              </SectionHeader>
              <Divider text="Total Privacy" />
              Your data is encrypted locally. By the time it hits a server, it&apos;s unreadable to
              everyone but you. We couldn't look at your files even if we wanted to.
              <Divider text="Modern Sharing" />
              Forget the "Create an Account" hurdles. Share instantly using your digital handle.
              Your files, your rules—grant or revoke access whenever you want.
              <div className="border-border-accent/40 my-6 border-t" />
              <TextLink href="/docs/at-protocol">
                Read the technical documentation <ArrowRightIcon size="1em" />
              </TextLink>
            </>
          }
        >
          <InfoCard icon="lock" title="End-to-end encrypted">
            Your files are encrypted before they leave your device. Only you hold the
            keys&nbsp;&mdash; always.
          </InfoCard>

          <InfoCard icon="network" title="No Platform Lock-in">
            Built on the same open standard as Bluesky. Your identity and your files belong to you,
            not us.
          </InfoCard>

          <InfoCard icon="share" title="Seamless sharing">
            Skip the "Create an Account" forms. Share instantly using your digital handle and stay
            in control of who sees what
          </InfoCard>

          <InfoCard icon="sparkles" title="We Can’t Peek">
            Our system is physically unable to read your data. Your privacy is built into the code,
            not just a promise in a legal document.
          </InfoCard>
        </InfoGrid>
      </Section>
      <Section id="how-it-works" surface="raised">
        <Divider text="How it works" />
        <SectionHeader>
          Simple for you. <Highlight>Invisible to everyone else</Highlight>.
        </SectionHeader>

        <StepGrid>
          <StepCard num="I" icon="lock" featured>
            <StepTitle>Start with your handle</StepTitle>
            <StepBody>
              Log in using your AT Protocol identity (like Bluesky). Opake connects to your digital
              home and sets up your private keys automatically.
            </StepBody>
          </StepCard>

          <StepCard num="II" icon="lock">
            <StepTitle>Automatic Privacy</StepTitle>
            <StepBody>
              Drop a file in. It’s locked on your device before it’s ever uploaded. To the rest of
              the world—including your storage provider—it’s completely unreadable.
            </StepBody>
          </StepCard>

          <StepCard num="III" icon="share">
            <StepTitle>Effortless Sharing</StepTitle>
            <StepBody>
              Just type a friend’s handle to share. We handle the complex security in the background
              so that only the person you chose can open what you’ve sent.
            </StepBody>
          </StepCard>

          <StepCard num="IV" icon="globe">
            <StepTitle>Never Locked In</StepTitle>
            <StepBody>
              You’re in charge of where your files live. Switch providers or host them yourself
              whenever you like. Your data and your identity always stay with you
            </StepBody>
          </StepCard>
        </StepGrid>
      </Section>

      <CenterAction
        headline="your cabinet is waiting."
        subtext="Take back your data. No surveillance, no compromise. Just your files, exactly as private as you choose."
      >
        <Link
          to="/cabinet"
          className="bg-base-100 text-base-content hover:bg-accent inline-flex items-center gap-2.5 rounded-lg px-7 py-3.5 text-sm font-medium transition-colors"
        >
          Open your cabinet <ArrowRightIcon size="1em" />
        </Link>
      </CenterAction>
    </>
  );
}

export const Route = createFileRoute("/_public/")({
  head: () => ({
    meta: ogMeta({
      title: "Opake — Your data, freely shared, privately kept",
      description: DESCRIPTION,
    }),
  }),
  component: LandingPage,
});
