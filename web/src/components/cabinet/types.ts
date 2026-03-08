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
}
