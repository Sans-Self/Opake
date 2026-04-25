export type EncStatus = "private" | "shared" | "public";

export type FileType = "document" | "spreadsheet" | "pdf" | "image" | "code" | "note" | "archive";

/** Optimistic proposal state for workspace member mutations. */
export type ProposalKind = "pending-add" | "pending-remove" | "pending-move" | "pending-update";

export interface ProposalInfo {
  readonly kind: ProposalKind;
  readonly authorDid: string;
  /** Directory this proposal targets (where it should appear or be removed from). */
  readonly targetDirectory: string;
}

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
  /** Present when this item is an unaccepted proposal from a workspace member. */
  proposal?: ProposalInfo;
}

const PREVIEWABLE_FILE_TYPES: ReadonlySet<FileType> = new Set(["image", "note"]);

/** Whether a file item can be previewed inline (images, markdown). */
export function isPreviewable(item: FileItem): boolean {
  return (
    item.kind === "file" &&
    item.decrypted &&
    !!item.fileType &&
    PREVIEWABLE_FILE_TYPES.has(item.fileType)
  );
}

/** Whether a file item can be opened in the markdown editor. */
export function isEditable(item: FileItem): boolean {
  return item.kind === "file" && item.decrypted && item.fileType === "note";
}
