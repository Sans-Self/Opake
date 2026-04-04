import { useCallback, useState } from "react";
import { useAuthStore } from "@/stores/auth";

type Phase = "choose" | "entering";

interface SeedPhraseRecovery {
  readonly phase: Phase;
  readonly loading: boolean;
  readonly error: string | null;
  readonly startEntering: () => void;
  readonly cancelEntering: () => void;
  readonly handleSubmit: (phrase: string) => void;
}

/** Shared recovery logic for RecoverIdentityView and ConflictView. */
export function useSeedPhraseRecovery(): SeedPhraseRecovery {
  const [phase, setPhase] = useState<Phase>("choose");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = useCallback(async (phrase: string) => {
    setLoading(true);
    setError(null);
    try {
      await useAuthStore.getState().saveIdentity(phrase);
      // If keys don't match remote, identity state will be "conflict"
      // and the parent view switches to ConflictView automatically.
      // If they match, identity → "ready".
    } catch (e) {
      setError(e instanceof Error ? e.message : "Recovery failed");
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    phase,
    loading,
    error,
    startEntering: useCallback(() => setPhase("entering"), []),
    cancelEntering: useCallback(() => setPhase("choose"), []),
    handleSubmit: useCallback((phrase: string) => void handleSubmit(phrase), [handleSubmit]),
  };
}
