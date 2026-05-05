// Display formatting utilities for your cabinet UI.

import type { FileType } from "@/components/cabinet/types";

const MIME_TO_FILE_TYPE: ReadonlyMap<string, FileType> = new Map([
  ["application/pdf", "pdf"],
  ["text/markdown", "note"],
  ["text/plain", "document"],
  ["text/csv", "spreadsheet"],
  ["application/json", "code"],
  ["application/javascript", "code"],
  ["text/javascript", "code"],
  ["text/typescript", "code"],
  ["text/html", "code"],
  ["text/css", "code"],
  ["text/xml", "code"],
  ["application/xml", "code"],
  ["application/zip", "archive"],
  ["application/gzip", "archive"],
  ["application/x-tar", "archive"],
  ["application/x-7z-compressed", "archive"],
  ["application/x-rar-compressed", "archive"],
]);

const MIME_PREFIX_TO_FILE_TYPE: ReadonlyMap<string, FileType> = new Map([
  ["image/", "image"],
  ["application/vnd.openxmlformats-officedocument.spreadsheetml", "spreadsheet"],
  ["application/vnd.ms-excel", "spreadsheet"],
  ["application/vnd.openxmlformats-officedocument.wordprocessingml", "document"],
  ["application/msword", "document"],
  ["application/vnd.openxmlformats-officedocument.presentationml", "document"],
]);

/** Map a MIME type to a cabinet FileType category. */
export function mimeTypeToFileType(mime: string): FileType {
  const exact = MIME_TO_FILE_TYPE.get(mime);
  if (exact) return exact;

  const prefixMatch = [...MIME_PREFIX_TO_FILE_TYPE.entries()].find(([prefix]) =>
    mime.startsWith(prefix),
  );

  return prefixMatch ? prefixMatch[1] : "document";
}

const SIZE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** Format byte count as human-readable size (e.g. 1048576 → "1 MB"). */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";

  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), SIZE_UNITS.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  const formatted = exponent === 0 ? value.toString() : value.toFixed(value < 10 ? 1 : 0);

  return `${formatted} ${SIZE_UNITS[exponent]}`;
}

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

/** Format an ISO datetime as a relative or short date string. */
export function formatRelativeDate(iso: string): string {
  const then = new Date(iso);
  const now = Date.now();
  const delta = now - then.getTime();

  if (delta < MINUTE_MS) return "Just now";
  if (delta < HOUR_MS) {
    const minutes = Math.floor(delta / MINUTE_MS);
    return `${minutes} min ago`;
  }
  if (delta < DAY_MS) {
    const hours = Math.floor(delta / HOUR_MS);
    return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
  }
  if (delta < 2 * DAY_MS) return "Yesterday";
  if (delta < 7 * DAY_MS) {
    const days = Math.floor(delta / DAY_MS);
    return `${days} days ago`;
  }

  return then.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

/** Truncate a DID for display, keeping prefix and abbreviated identifier. */
export function truncateDid(did: string): string {
  const lastColon = did.lastIndexOf(":");
  if (lastColon === -1) return did;
  const prefix = did.slice(0, lastColon + 1);
  const id = did.slice(lastColon + 1);
  if (id.length <= 8) return did;
  return `${prefix}${id.slice(0, 4)}…${id.slice(-3)}`;
}

/**
 * Format an ISO date as a short locale string (e.g. "Mar 8, 2026").
 *
 * `fallback` is returned for empty strings and unparseable inputs.
 * `toLocaleDateString` does not throw on invalid dates — it returns
 * "Invalid Date" — so the empty-string + `Number.isNaN` guards here
 * are doing the real work, not a try/catch.
 */
export function formatShortDate(iso: string, fallback = "unknown date"): string {
  if (!iso) return fallback;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return fallback;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

const DOCUMENT_COLLECTION = "app.opake.document";
const DIRECTORY_COLLECTION = "app.opake.directory";

/** Determine item kind from an AT-URI's collection segment. */
export function entryKindFromUri(uri: string): "file" | "folder" {
  if (uri.includes(DIRECTORY_COLLECTION)) return "folder";
  if (uri.includes(DOCUMENT_COLLECTION)) return "file";
  return "file";
}
