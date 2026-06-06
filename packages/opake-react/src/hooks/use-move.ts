import type { MutationResult } from "@opake/sdk";
import { useTreeMutation } from "./use-tree-mutation";

interface MoveInput {
  readonly entryUri: string;
  readonly sourceDirUri: string;
  readonly targetDirUri: string;
}

/**
 * Move an entry (document or directory) between directories.
 *
 * Optimistic: the entry moves between directories immediately.
 *
 * @param keyringUri - Pass null for cabinet, or a workspace keyring URI.
 */
export function useMove(keyringUri: string | null) {
  return useTreeMutation<MoveInput, MutationResult>({
    keyringUri,
    mutationFn: (fm, input) => fm.move(input.entryUri, input.sourceDirUri, input.targetDirUri),
    optimisticUpdate: (snapshot, input) => {
      const source = snapshot.directories[input.sourceDirUri];
      const target = snapshot.directories[input.targetDirUri];
      if (!source || !target) return snapshot;

      const movedEntry = source.entries.find((e) => e.uri === input.entryUri);
      if (!movedEntry) return snapshot;

      return {
        ...snapshot,
        directories: {
          ...snapshot.directories,
          [input.sourceDirUri]: {
            ...source,
            entries: source.entries.filter((e) => e.uri !== input.entryUri),
          },
          [input.targetDirUri]: {
            ...target,
            entries: [...target.entries, movedEntry],
          },
        },
      };
    },
    // Release once the echo shows the entry settled at its destination:
    // present in the target directory and gone from the source. Holding
    // for both halves avoids the entry snapping back mid-flight.
    buildSettlePredicate: (input) => (base) => {
      const inTarget = base.directories[input.targetDirUri]?.entries.some((e) => e.uri === input.entryUri) ?? false;
      const goneFromSource = !(base.directories[input.sourceDirUri]?.entries.some((e) => e.uri === input.entryUri) ?? false);
      return inTarget && goneFromSource;
    },
  });
}
