import { useCallback, useState } from "react";
import { useAuthStore } from "@/stores/auth";

type Phase = "choose" | "entering" | "mismatch";

interface SeedPhraseRecovery {
  readonly phase: Phase;
  readonly loading: boolean;
  readonly error: string | null;
  readonly startEntering: () => void;
  readonly cancelEntering: () => void;
  readonly handleSubmit: (phrase: string) => void;
  readonly handleForceRecover: () => void;
}

/** Shared recovery logic for RecoverIdentityView and ConflictView. */
export function useSeedPhraseRecovery(): SeedPhraseRecovery {
  const [phase, setPhase] = useState<Phase>("choose");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingPhrase, setPendingPhrase] = useState<string | null>(null);

  const handleSubmit = useCallback(async (phrase: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await useAuthStore.getState().recoverFromSeedPhrase(phrase);
      if (result.mismatch) {
        setPendingPhrase(phrase);
        setPhase("mismatch");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Recovery failed");
    } finally {
      setLoading(false);
    }
  }, []);

  const handleForceRecover = useCallback(async () => {
    if (!pendingPhrase) return;
    setLoading(true);
    setError(null);
    try {
      await useAuthStore.getState().recoverFromSeedPhrase(pendingPhrase, true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Recovery failed");
    } finally {
      setLoading(false);
    }
  }, [pendingPhrase]);

  return {
    phase,
    loading,
    error,
    startEntering: useCallback(() => setPhase("entering"), []),
    cancelEntering: useCallback(() => setPhase("choose"), []),
    handleSubmit: useCallback((phrase: string) => void handleSubmit(phrase), [handleSubmit]),
    handleForceRecover: useCallback(() => void handleForceRecover(), [handleForceRecover]),
  };
}
