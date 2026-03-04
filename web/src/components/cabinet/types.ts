export type EncStatus = "private" | "shared" | "public";

export type FileType =
  | "document"
  | "spreadsheet"
  | "pdf"
  | "image"
  | "code"
  | "note"
  | "archive";

export interface FileItem {
  id: string;
  name: string;
  kind: "file" | "folder";
  fileType?: FileType;
  encrypted: boolean;
  status: EncStatus;
  sharedWith?: string[];
  size?: string;
  items?: number;
  modified: string;
  starred: boolean;
}

export type PanelType =
  | "root"
  | "folder"
  | "shared"
  | "starred"
  | "encrypted"
  | "docs"
  | "trash"
  | "settings";

export interface Panel {
  id: string;
  type: PanelType;
  title: string;
  data?: FileItem;
}
