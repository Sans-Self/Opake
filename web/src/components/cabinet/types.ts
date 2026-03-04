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

export type SectionType =
  | "root"
  | "shared"
  | "starred"
  | "encrypted"
  | "docs"
  | "trash"
  | "settings";

export type PanelType = SectionType | "folder";

export type Panel =
  | { type: "folder"; folderId: string; title: string; itemCount?: number }
  | { type: SectionType; title: string };

export function panelKey(panel: Panel): string {
  return panel.type === "folder" ? panel.folderId : panel.type;
}
