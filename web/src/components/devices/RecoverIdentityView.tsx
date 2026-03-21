import { useCallback, useState } from "react";
import { ArrowsLeftRightIcon, AmbulanceIcon, TrashIcon, WarningIcon } from "@phosphor-icons/react";
import { useAuthStore } from "@/stores/auth";
import { DestructiveConfirmation } from "@/components/DestructiveConfirmation";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";
import { SeedPhraseDisplay } from "./SeedPhraseDisplay";
import { SeedPhraseInput } from "./SeedPhraseInput";
import { SeedPhraseMismatchWarning } from "./SeedPhraseMismatchWarning";
import { useSeedPhraseRecovery } from "./useSeedPhraseRecovery";

type FreshPhase = "confirming" | "generating" | "showing" | "saving" | "error";

export function RecoverIdentityView() {
  const recovery = useSeedPhraseRecovery();
  const [freshPhase, setFreshPhase] = useState<FreshPhase | null>(null);
  const [freshPhrase, setFreshPhrase] = useState<string | null>(null);
  const [freshError, setFreshError] = useState<string | null>(null);

  const handleStartFresh = useCallback(async () => {
    setFreshPhase("generating");
    const mnemonic = await useAuthStore.getState().generateSeedPhrase();
    setFreshPhrase(mnemonic);
    setFreshPhase("showing");
  }, []);

  const handleFreshConfirmed = useCallback(async () => {
    if (!freshPhrase) return;
    setFreshPhase("saving");
    try {
      await useAuthStore.getState().confirmSeedPhrase(freshPhrase);
      // Identity → "ready", parent switches to ReadyView
    } catch (e) {
      setFreshError(e instanceof Error ? e.message : "Failed to publish key");
      setFreshPhase("error");
    }
  }, [freshPhrase]);

  // Seed phrase recovery sub-views
  if (recovery.phase === "mismatch") {
    return (
      <SeedPhraseMismatchWarning
        onConfirm={recovery.handleForceRecover}
        onCancel={recovery.cancelEntering}
        loading={recovery.loading}
        error={recovery.error}
      />
    );
  }

  if (recovery.phase === "entering") {
    return (
      <SeedPhraseInput
        onSubmit={recovery.handleSubmit}
        onCancel={recovery.cancelEntering}
        loading={recovery.loading}
        error={recovery.error}
      />
    );
  }

  // "Start fresh" danger confirmation — type phrase to confirm
  if (freshPhase === "confirming") {
    return (
      <div className="flex flex-col items-center gap-6 text-center">
        <PageHeader
          icon={WarningIcon}
          iconClassName="text-error"
          title="Start fresh?"
          description="This will generate a new encryption key and overwrite the one on your account."
        />

        <div className="bg-base-200 max-w-md rounded-lg p-4 text-left text-sm">
          <p className="text-base-content/70 font-medium">You will permanently lose access to:</p>
          <ul className="text-base-content/60 mt-2 list-inside list-disc space-y-1">
            <li>All files encrypted with the current key</li>
            <li>All shared files and workspace memberships</li>
            <li>Any grants others have created for you</li>
          </ul>
        </div>

        <DestructiveConfirmation
          phrase="This will lock me out of my Opake data and I am okay with that"
          onConfirm={() => void handleStartFresh()}
        />

        <button onClick={() => setFreshPhase(null)} className="btn btn-ghost btn-sm">
          Go back
        </button>
      </div>
    );
  }

  // "Start fresh" generating/saving
  if (freshPhase === "generating" || freshPhase === "saving") {
    return (
      <div className="flex flex-col items-center gap-4 text-center">
        <PageHeader
          title={
            freshPhase === "generating" ? "Generating seed phrase..." : "Setting up encryption..."
          }
        />
      </div>
    );
  }

  // "Start fresh" error
  if (freshPhase === "error") {
    return (
      <div className="flex flex-col items-center gap-6 text-center">
        <PageHeader
          icon={WarningIcon}
          iconClassName="text-error"
          title="Something went wrong"
          description={freshError ?? "Failed to set up encryption key."}
        />
        <button onClick={() => void handleFreshConfirmed()} className="btn btn-primary btn-sm">
          Try again
        </button>
      </div>
    );
  }

  // "Start fresh" — show seed phrase, publish on confirm
  if (freshPhase === "showing" && freshPhrase) {
    return (
      <SeedPhraseDisplay phrase={freshPhrase} onConfirmed={() => void handleFreshConfirmed()} />
    );
  }

  // Main choice screen
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        title="Welcome back"
        description="You're already set up on another device. Bring your key here to access your files."
      />

      <div className="flex w-full flex-wrap justify-center-safe gap-4">
        <ChoiceButton
          as="Link"
          to="/devices/pair/request"
          icon={ArrowsLeftRightIcon}
          title="Copy from another device (recommended)"
          description="Open Opake on a device you're already using and approve the transfer."
        />

        <ChoiceButton
          as="Button"
          onClick={recovery.startEntering}
          icon={AmbulanceIcon}
          title="Use your recovery phrase"
          description="Enter the 24 words you saved when you first set up."
        />

        <ChoiceButton
          as="Button"
          onClick={() => setFreshPhase("confirming")}
          icon={TrashIcon}
          title="Start fresh (destructive)"
          description="Generate a new key. You'll lose access to all existing encrypted files."
          className="border-error/30 hover:border-error/60"
        />
      </div>
    </div>
  );
}
