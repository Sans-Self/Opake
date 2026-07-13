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
