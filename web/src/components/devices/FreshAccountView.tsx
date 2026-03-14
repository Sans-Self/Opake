import { useCallback, useState } from "react";
import { useAuthStore } from "@/stores/auth";
import { KeyIcon, WarningIcon } from "@phosphor-icons/react";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";
import { SeedPhraseDisplay } from "./SeedPhraseDisplay";

type Phase = "idle" | "generating" | "showing" | "saving" | "error";

export function FreshAccountView() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [phrase, setPhrase] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleStart = useCallback(async () => {
    setPhase("generating");
    const mnemonic = await useAuthStore.getState().generateSeedPhrase();
    setPhrase(mnemonic);
    setPhase("showing");
  }, []);

  const handleConfirmed = useCallback(async () => {
    if (!phrase) return;
    setPhase("saving");
    try {
      await useAuthStore.getState().confirmSeedPhrase(phrase);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to publish key");
      setPhase("error");
    }
  }, [phrase]);

  if (phase === "error") {
    return (
      <div className="flex flex-col items-center gap-6 text-center">
        <PageHeader
          icon={WarningIcon}
          iconClassName="text-error"
          title="Something went wrong"
          description={error ?? "Failed to set up encryption key."}
        />
        <button onClick={() => void handleConfirmed()} className="btn btn-primary btn-sm">
          Try again
        </button>
      </div>
    );
  }

  if (phase === "idle") {
    return (
      <div className="flex flex-col items-center gap-6 text-center">
        <PageHeader
          title="Welcome to Opake"
          description="To keep your files private, Opake needs to create an encryption key for this device."
        />
        <ChoiceButton
          as="Button"
          onClick={() => void handleStart()}
          icon={KeyIcon}
          title="Create my key"
          description="This only takes a moment."
        />
      </div>
    );
  }

  if (phase === "generating" || phase === "saving") {
    return (
      <div className="flex flex-col items-center gap-4 text-center">
        <PageHeader
          title={phase === "generating" ? "Generating seed phrase..." : "Setting up encryption..."}
        />
      </div>
    );
  }

  if (!phrase) return null;
  return <SeedPhraseDisplay phrase={phrase} onConfirmed={() => void handleConfirmed()} />;
}
