import type { NameHydrationState } from "@opake/react";

export type EncStatus = "private" | "shared" | "public";

export type FileType = "document" | "spreadsheet" | "pdf" | "image" | "code" | "note" | "archive";

export interface FileItem {
  id: string;
  uri: string;
  name: string;
  kind: "file" | "folder";
  fileType?: FileType;
  encrypted: boolean;
  status: EncStatus;
  size?: string;
  items?: number;
  modified: string;
  decrypted: boolean;
  tags: string[];
  mimeType?: string;
  description?: string;
  subtitle?: string;
  /**
   * First-class provisional marker. `true` for an entry an operation-scoped
   * optimistic overlay is showing ahead of the indexer's echo (an uploading
   * document, a just-created directory). A pending entry is non-actionable
   * and visibly pending, and self-retracts when the echo lands — it is never
   * an authorization belief, a dependent-operation input, or a mutation
   * target. Set explicitly from the overlay's marker, never inferred from URI
   * shape or missing metadata.
   */
  pending?: boolean;
  /**
   * Name-hydration state for an undecrypted file row. Only meaningful when
   * `decrypted` is false and `pending` is unset: it splits the single
   * "Decrypting…" placeholder into `resolving` (in progress), `retryable`
   * (transient resolve failure, a manual retry may help), and `undecryptable`
   * (definitive — this caller has no key). Absent for folders and resolved
   * files, which are always `resolved`.
   */
  hydration?: NameHydrationState;
}

const PREVIEWABLE_FILE_TYPES: ReadonlySet<FileType> = new Set(["image", "note"]);

/** Whether a file item can be previewed inline (images, markdown). */
export function isPreviewable(item: FileItem): boolean {
  return (
    !item.pending &&
    item.kind === "file" &&
    item.decrypted &&
    !!item.fileType &&
    PREVIEWABLE_FILE_TYPES.has(item.fileType)
  );
}

/** Whether a file item can be opened in the markdown editor. */
export function isEditable(item: FileItem): boolean {
  return !item.pending && item.kind === "file" && item.decrypted && item.fileType === "note";
}

/** How an undecrypted file row presents its name-hydration state. */
export interface HydrationPresentation {
  /** Accessible label / tooltip for the row while the name is unresolved. */
  readonly label: string;
  /** Whether the row should advertise `aria-busy` (resolve in progress). */
  readonly busy: boolean;
  /** Whether to offer a manual retry affordance (retryable failure). */
  readonly retryable: boolean;
}

/**
 * Map an undecrypted file's hydration state to its row presentation. Splits
 * the former single "Decrypting…" placeholder into distinct, accessible
 * states so a stuck name reads as a retryable failure — not a permanent,
 * unexplained spinner — and a no-key document reads as definitive.
 */
export function hydrationPresentation(item: FileItem): HydrationPresentation {
  switch (item.hydration ?? "resolving") {
    case "retryable":
      return { label: "Name unavailable — retry", busy: false, retryable: true };
    case "undecryptable":
      return { label: "Encrypted — no access", busy: false, retryable: false };
    case "resolving":
    case "resolved":
      return { label: "Decrypting…", busy: true, retryable: false };
  }
}

/**
 * Whether an entry can be the target of any user operation (open, edit,
 * rename, move, delete, share). A provisional (pending) entry never can — it
 * has no indexer-visible record behind it yet, so acting on it would target a
 * placeholder URI. Folders and decrypted files are otherwise actionable;
 * an undecrypted (metadata-pending) file stays inert until its metadata lands.
 */
export function isActionable(item: FileItem): boolean {
  if (item.pending) return false;
  return item.kind === "folder" || item.decrypted;
}
