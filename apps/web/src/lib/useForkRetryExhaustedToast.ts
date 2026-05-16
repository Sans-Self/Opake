// Cross-mutation chain-fork exhaustion → toast bridge.
//
// Every `useTreeMutation`-derived hook (useUpload, useDelete, etc.)
// exposes `forkRetryState` + `dismissForkRetry`. When a mutation's
// retry budget exhausts, the call site is responsible for surfacing
// it. Most surfaces have several mutations active at once (a file
// browser has six: upload + delete + delete-dir + create-dir + rename
// + move), and rather than wire a useEffect per hook we collapse the
// fan-in here.
//
// Behaviour: on transition `idle | retrying → exhausted` for any
// hook in the list, fire one toast and dismiss the exhausted state.
// Multiple simultaneous exhaustions still produce one toast (the
// user has lost a series of races; they don't need N copies of the
// same banner).

import { useEffect } from "react";
import type { TreeMutationResult } from "@opake/react";
import { toastError } from "@/stores/toast";

// `TreeMutationResult<unknown, unknown>` doesn't compile because of
// React Query's correlated unions over the result generics. We only
// touch the two federation-specific fields, so a structural slice is
// all we need.
type ForkRetryView = Pick<
  TreeMutationResult<unknown, unknown>,
  "forkRetryState" | "dismissForkRetry"
>;

const EXHAUSTED_MESSAGE =
  "Your last change kept losing a race against another contributor. " +
  "We retried it a few times — please try again when the workspace settles.";

export function useForkRetryExhaustedToast(mutations: readonly ForkRetryView[]): void {
  // Reduce the list to "is anyone exhausted right now." The dep array
  // captures the boolean rather than the mutation array so re-renders
  // that don't change the fork-retry signal don't re-fire the effect.
  const anyExhausted = mutations.some((m) => m.forkRetryState === "exhausted");

  useEffect(() => {
    if (!anyExhausted) return;
    toastError(EXHAUSTED_MESSAGE);
    // Dismiss every exhausted slot so the next mutation starts clean
    // and we don't loop on the same banner. Mutations that aren't
    // exhausted are no-ops under dismiss.
    mutations.forEach((m) => {
      if (m.forkRetryState === "exhausted") m.dismissForkRetry();
    });
    // `mutations` is a fresh array on every render; depending on it
    // would re-fire constantly. The boolean is the actual signal.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anyExhausted]);
}
