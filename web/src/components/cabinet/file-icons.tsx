import {
  Folder,
  FileText,
  File,
  BookOpen,
  Archive,
} from "@phosphor-icons/react";
import type { FileItem } from "./types";

interface IconStyle {
  bg: string;
  text: string;
}

const FOLDER_STYLE: IconStyle = { bg: "bg-accent", text: "text-primary" };

const FILE_TYPE_STYLES: Record<string, IconStyle> = {
  document: { bg: "bg-file-doc-bg", text: "text-file-doc" },
  spreadsheet: { bg: "bg-file-sheet-bg", text: "text-file-sheet" },
  pdf: { bg: "bg-file-pdf-bg", text: "text-file-pdf" },
  note: { bg: "bg-accent", text: "text-file-note" },
  code: { bg: "bg-file-code-bg", text: "text-file-code" },
  archive: { bg: "bg-bg-stone", text: "text-text-muted" },
};

const DEFAULT_STYLE: IconStyle = { bg: "bg-bg-stone", text: "text-text-muted" };

export function fileIconColors(item: FileItem): IconStyle {
  if (item.kind === "folder") return FOLDER_STYLE;
  return FILE_TYPE_STYLES[item.fileType ?? ""] ?? DEFAULT_STYLE;
}

export function fileIconElement(item: FileItem, size = 15) {
  if (item.kind === "folder") return <Folder size={size} weight="fill" />;
  switch (item.fileType) {
    case "document":
      return <FileText size={size} />;
    case "spreadsheet":
      return <File size={size} />;
    case "note":
      return <BookOpen size={size} />;
    case "archive":
      return <Archive size={size} />;
    default:
      return <File size={size} />;
  }
}
