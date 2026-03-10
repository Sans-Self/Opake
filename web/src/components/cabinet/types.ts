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
