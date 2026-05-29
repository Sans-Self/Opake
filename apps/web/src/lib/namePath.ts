// Name-path URL resolution for the cabinet/workspace file browser.
//
// URLs identify directories by the path of decrypted names from root to
// leaf, not by directory record rkey. This decouples the URL contract
// from the chain-supersede cascade — a mutation that rewrites every
// ancestor's rkey leaves the path-of-names unchanged, so URLs survive
// any tree mutation that doesn't rename or move an ancestor.
//
// Identity for matching: NFC-normalized, byte-equal. Comparison is
// case-sensitive — `Photos` and `photos` are distinct, matching the
// POSIX filesystem convention and avoiding collation rules.
//
// Encoding: pure UTF-8 strings; TanStack Router and CLI argv both
// handle UTF-8 natively. Wire form (`%C3%A9` in the actual URL string)
// is a transit detail; this module never percent-encodes.

import type { DirectoryInfo, DirectoryTreeSnapshot } from "./pdsTypes";

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/** Maximum name length, in UTF-8 bytes. Matches POSIX NAME_MAX. */
export const MAX_NAME_BYTES = 255;

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/**
 * NFC-normalize and trim leading/trailing whitespace from a candidate name.
 *
 * Normalization happens at the write boundary and at URL-parse time so
 * comparisons are byte-equal. Inner whitespace is preserved.
 *
 * Trimming is opinionated: leading/trailing whitespace is overwhelmingly
 * user error (slipped paste, mis-typed shift), never intentional.
 */
export function normalizeName(raw: string): string {
  return raw.normalize("NFC").trim();
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

export type NameValidationReason =
  | "empty"
  | "reserved"
  | "forbidden-char"
  | "too-long";

export type NameValidation =
  | { readonly ok: true; readonly normalized: string }
  | { readonly ok: false; readonly reason: NameValidationReason };

// Reserved name set covers POSIX path-resolution ambiguity (`.`/`..`)
// plus the file-leaf URL sentinel `FILE_MARKER`. The latter is reserved
// here so the URL grammar (`<dirs>/<MARKER>/<filename>`) has an
// unambiguous structural separator — no legitimate name can collide
// with the marker, so the lastIndexOf-based parser is sound for every
// input the validator accepts.
const RESERVED_NAMES: ReadonlySet<string> = new Set([".", "..", "__file__"]);
const FORBIDDEN_CHARS = /[/\0\n\r\t]/;

/**
 * Validate a candidate name and return the normalized form on success.
 *
 * Rejection set:
 *   * empty (after trim)
 *   * "." or ".." — ambiguous in path resolution
 *   * `/` — fatal because it's the URL segment separator
 *   * `\0`, `\n`, `\r`, `\t` — break list display, never appear in legitimate names
 *   * length > 255 UTF-8 bytes — POSIX NAME_MAX baseline
 *
 * Spaces, punctuation, and all non-Latin scripts pass through. The
 * forbidden set deliberately excludes Windows-style reservations
 * (`<>:"|?*`) — we control our own resolver, those characters work
 * in our URLs once routed through TanStack Router.
 */
export function validateName(raw: string): NameValidation {
  const normalized = normalizeName(raw);

  if (normalized.length === 0) {
    return { ok: false, reason: "empty" };
  }

  if (RESERVED_NAMES.has(normalized)) {
    return { ok: false, reason: "reserved" };
  }

  if (FORBIDDEN_CHARS.test(normalized)) {
    return { ok: false, reason: "forbidden-char" };
  }

  if (new TextEncoder().encode(normalized).byteLength > MAX_NAME_BYTES) {
    return { ok: false, reason: "too-long" };
  }

  return { ok: true, normalized };
}

/** Human-readable explanation for a validation failure. */
export function describeValidationReason(reason: NameValidationReason): string {
  switch (reason) {
    case "empty":
      return "Name cannot be empty";
    case "reserved":
      return `"." and ".." are reserved`;
    case "forbidden-char":
      return "Name cannot contain `/`, line breaks, or null bytes";
    case "too-long":
      return `Name exceeds ${String(MAX_NAME_BYTES)}-byte limit`;
  }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/**
 * Walk a sequence of directory names from root to leaf, returning the
 * URI of the resolved directory. Returns null if any segment doesn't
 * match a child directory at its level.
 *
 * Comparison is NFC-byte-equal — callers must normalize URL segments
 * before passing them in (or accept normalization happens here).
 *
 * Document names are ignored: paths address directories only. A document
 * named "projects" does not match a `/projects/...` URL.
 */
export function resolveDirectoryFromNamePath(
  snapshot: DirectoryTreeSnapshot,
  names: readonly string[],
): string | null {
  if (names.length === 0 || !snapshot.rootUri) return null;

  return names.reduce<string | null>((currentUri, rawName) => {
    if (!currentUri) return null;
    const dir = snapshot.directories[currentUri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
    if (!dir) return null;

    const target = normalizeName(rawName);
    const child = dir.entries.find((entry) => {
      if (entry.type !== "directory") return false;
      const info = snapshot.directories[entry.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      return info ? normalizeName(info.name) === target : false;
    });

    return child?.uri ?? null;
  }, snapshot.rootUri);
}

/**
 * Build the sequence of names from root → directory. Returns null for
 * the root itself or a missing URI — callers fall back to the base path.
 * Names are NFC-normalized.
 */
export function directoryNamePathSegments(
  snapshot: DirectoryTreeSnapshot,
  directoryUri: string | null,
): readonly string[] | null {
  if (!directoryUri || directoryUri === snapshot.rootUri) return null;
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
  if (!snapshot.directories[directoryUri]) return null;

  // Recursively gather names from leaf → root, then reverse. Pure
  // recursion lets each call return its slice of the answer without
  // mutating a shared accumulator. A visited set guards against
  // pathological cycles (shouldn't happen on a well-formed tree, but
  // a bad snapshot crashing the URL builder would be invisible).
  const collectUp = (
    uri: string | null,
    visited: ReadonlySet<string>,
  ): readonly string[] | null => {
    if (!uri || uri === snapshot.rootUri) return [];
    if (visited.has(uri)) return null;
    const info: DirectoryInfo | undefined = snapshot.directories[uri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
    if (!info) return null;
    const above = collectUp(info.parentUri, new Set([...visited, uri]));
    if (above === null) return null;
    return [...above, normalizeName(info.name)];
  };

  return collectUp(directoryUri, new Set());
}

/**
 * Convenience over [`directoryNamePathSegments`]: return a `/`-joined
 * URL suffix for the path-from-root, or null for root / missing URI.
 * Suitable as a TanStack splat value or for direct interpolation.
 */
export function directoryNamePathSuffix(
  snapshot: DirectoryTreeSnapshot,
  directoryUri: string | null,
): string | null {
  const segments = directoryNamePathSegments(snapshot, directoryUri);
  return segments ? segments.join("/") : null;
}

/**
 * Walk as many name segments as possible from root, stopping at the first
 * segment that doesn't resolve. Used by the "not found" UI to surface
 * the deepest reachable directory (the "go to parent" target) and the
 * specific segment that failed.
 *
 * Properties:
 *   * `resolvedDepth === names.length`  ⇒ full path resolved.
 *   * `resolvedDepth === 0`            ⇒ first segment failed (or root missing).
 *   * `0 < resolvedDepth < names.length` ⇒ walk stopped at `names[resolvedDepth]`.
 *
 * `resolvedUri` is the URI of the deepest reached directory (root when
 * `resolvedDepth === 0`, deeper otherwise). Null only when the snapshot
 * has no root at all.
 */
export interface PartialResolution {
  readonly resolvedDepth: number;
  readonly resolvedUri: string | null;
}

export function partialResolveNamePath(
  snapshot: DirectoryTreeSnapshot,
  names: readonly string[],
): PartialResolution {
  if (!snapshot.rootUri) return { resolvedDepth: 0, resolvedUri: null };

  // Reduce stops the walk by carrying a "done" flag — once the walk
  // hits a missing segment the accumulator pins the depth + URI of the
  // last reached directory and ignores subsequent iterations. Plain
  // reduce can't early-exit, but pinning the failure state is
  // semantically equivalent and keeps the body declarative.
  type Acc = { readonly depth: number; readonly uri: string; readonly done: boolean };
  const initial: Acc = { depth: 0, uri: snapshot.rootUri, done: false };

  const final = names.reduce<Acc>((acc, rawName, index) => {
    if (acc.done) return acc;
    const dir = snapshot.directories[acc.uri];
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
    if (!dir) return { ...acc, done: true };

    const target = normalizeName(rawName);
    const child = dir.entries.find((entry) => {
      if (entry.type !== "directory") return false;
      const info = snapshot.directories[entry.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      return info ? normalizeName(info.name) === target : false;
    });

    if (!child) return { ...acc, done: true };
    return { depth: index + 1, uri: child.uri, done: false };
  }, initial);

  return { resolvedDepth: final.depth, resolvedUri: final.uri };
}

// ---------------------------------------------------------------------------
// Splat parsing — `/dir/dir/f/filename` shape
// ---------------------------------------------------------------------------

/**
 * Marker segment that introduces a file leaf in a name-path URL.
 *
 * Must be a value that's reserved by [`validateName`] — otherwise a
 * file or directory could be legitimately named the same as the marker,
 * which would break the lastIndexOf-based parser. `__file__` is added
 * to `RESERVED_NAMES` for exactly this purpose: no validated write can
 * ever produce a tree entry whose decrypted name equals this string,
 * so any URL segment matching the marker is unambiguously structural.
 */
export const FILE_MARKER = "__file__";

export interface ParsedSplatPath {
  /** Directory segments leading to the leaf, NFC-normalized. */
  readonly dirSegments: readonly string[];
  /** File leaf name when the URL specified one, NFC-normalized; null otherwise. */
  readonly fileSegment: string | null;
}

/**
 * Parse a TanStack splat string into directory segments and an optional
 * file leaf. The grammar is:
 *
 *     splat ::= dirs                       — directory view
 *             | dirs "/" "f" "/" filename  — file leaf inside `dirs`
 *
 * `dirs` is a (possibly empty) `/`-joined sequence of directory names.
 * `filename` is exactly one segment; further segments after `filename`
 * are ignored (the parser stops at the first leaf segment).
 *
 * Empty or whitespace-only segments are dropped before the structure is
 * interpreted, so trailing slashes and double-slashes don't change the
 * meaning.
 */
export function parseSplatPath(splat: string | undefined): ParsedSplatPath {
  const raw = (splat ?? "").split("/").filter((s) => s.length > 0);
  // FILE_MARKER is in RESERVED_NAMES, so it cannot occur as a legitimate
  // segment value — its presence in the path is unambiguously the file-
  // leaf separator. The marker always lies one segment before the leaf.
  const markerIdx = raw.indexOf(FILE_MARKER);
  if (markerIdx === -1 || markerIdx === raw.length - 1) {
    return {
      dirSegments: raw.map(normalizeName),
      fileSegment: null,
    };
  }
  return {
    dirSegments: raw.slice(0, markerIdx).map(normalizeName),
    fileSegment: normalizeName(raw[markerIdx + 1]),
  };
}

/**
 * Build a TanStack splat value from directory segments and an optional
 * file leaf. Inverse of [`parseSplatPath`] for any well-formed input.
 */
export function buildSplatPath(
  dirSegments: readonly string[],
  fileSegment?: string | null,
): string {
  const dirs = dirSegments.map(normalizeName).join("/");
  if (!fileSegment) return dirs;
  const leaf = normalizeName(fileSegment);
  return dirs.length > 0 ? `${dirs}/${FILE_MARKER}/${leaf}` : `${FILE_MARKER}/${leaf}`;
}

// ---------------------------------------------------------------------------
// Uniqueness
// ---------------------------------------------------------------------------

export type NameConflict = {
  readonly uri: string;
  readonly type: "directory" | "document";
};

/** Map of document URI → its decrypted metadata (or at least the name). */
export type DocumentNameLookup = Readonly<Record<string, { readonly name: string } | undefined>>;

/**
 * Pluggable resolver for in-flight optimistic entries that haven't yet
 * materialized their decrypted metadata. The default returns null —
 * callers that maintain an optimistic overlay (e.g. uploads) pass a
 * function that decodes the entry's intended name from its URI.
 *
 * This indirection keeps `namePath.ts` free of any optimistic-overlay
 * coupling while still letting the conflict check see in-flight names.
 */
export type PendingNameResolver = (uri: string) => string | null;

/**
 * Find an entry in `parentUri` whose normalized name equals `candidate`.
 *
 * Document names are sourced from `documentMetadata`. When metadata for
 * a document is missing from the lookup (e.g. parent not yet warmed),
 * the caller-supplied `pendingNameResolver` gets a shot at the URI — an
 * optimistic upload placeholder, for example, encodes the intended
 * filename into its URI so a rapid double-click sees the in-flight name
 * as a conflict instead of slipping through.
 *
 * `excludeUri` allows a rename operation to exclude the entry being
 * renamed from conflict consideration. Lets `rename to same name` and
 * case-only changes work cleanly.
 */
export function findNameConflict(
  snapshot: DirectoryTreeSnapshot,
  parentUri: string,
  candidate: string,
  documentMetadata: DocumentNameLookup,
  excludeUri?: string,
  pendingNameResolver?: PendingNameResolver,
): NameConflict | null {
  const parent = snapshot.directories[parentUri];
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
  if (!parent) return null;

  const target = normalizeName(candidate);

  const nameOf = (entry: { readonly uri: string; readonly type: "directory" | "document" }): string | null => {
    if (entry.type === "directory") {
      const info = snapshot.directories[entry.uri];
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard: Record lookup
      return info ? normalizeName(info.name) : null;
    }
    const meta = documentMetadata[entry.uri];
    if (meta) return normalizeName(meta.name);
    const pending = pendingNameResolver?.(entry.uri);
    return pending !== null && pending !== undefined ? normalizeName(pending) : null;
  };

  const conflictEntry = parent.entries.find(
    (entry) => entry.uri !== excludeUri && nameOf(entry) === target,
  );

  return conflictEntry ? { uri: conflictEntry.uri, type: conflictEntry.type } : null;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class NameAlreadyExistsError extends Error {
  readonly existingUri: string;
  readonly existingType: "directory" | "document";
  readonly parentUri: string;
  readonly attemptedName: string;

  constructor(params: {
    readonly attemptedName: string;
    readonly parentUri: string;
    readonly conflict: NameConflict;
  }) {
    const kind = params.conflict.type === "directory" ? "folder" : "file";
    super(`A ${kind} named "${params.attemptedName}" already exists here`);
    this.name = "NameAlreadyExistsError";
    this.attemptedName = params.attemptedName;
    this.parentUri = params.parentUri;
    this.existingUri = params.conflict.uri;
    this.existingType = params.conflict.type;
  }
}

export class InvalidNameError extends Error {
  readonly reason: NameValidationReason;

  constructor(reason: NameValidationReason) {
    super(describeValidationReason(reason));
    this.name = "InvalidNameError";
    this.reason = reason;
  }
}

// ---------------------------------------------------------------------------
// Combined validation + availability check (the boundary callers want)
// ---------------------------------------------------------------------------

export type NameAvailability =
  | { readonly ok: true; readonly normalized: string }
  | { readonly ok: false; readonly message: string };

/**
 * One-shot name-and-availability check at a mutation boundary.
 *
 * Runs [`validateName`] for forbidden chars / length, then
 * [`findNameConflict`] against the target parent. Returns the
 * normalized name on success or a UI-ready message on failure
 * (already produced by the underlying helpers — no further
 * translation needed by callers).
 *
 * `documentMetadata` should hold whatever the caller has loaded for
 * the target parent. When the target parent's metadata isn't loaded
 * — typical for cross-folder operations — pass an empty object;
 * the check falls back to directory-name uniqueness only, and any
 * cross-folder document-name collision is left to the post-write
 * fork-retry surface.
 */
export function checkNameAvailability(params: {
  readonly snapshot: DirectoryTreeSnapshot | null;
  readonly parentUri: string;
  readonly rawName: string;
  readonly documentMetadata: DocumentNameLookup;
  readonly excludeUri?: string;
  readonly pendingNameResolver?: PendingNameResolver;
}): NameAvailability {
  const { snapshot, parentUri, rawName, documentMetadata, excludeUri, pendingNameResolver } =
    params;

  const validation = validateName(rawName);
  if (!validation.ok) {
    return { ok: false, message: describeValidationReason(validation.reason) };
  }
  if (!snapshot) {
    return { ok: false, message: "Tree not loaded yet" };
  }
  const conflict = findNameConflict(
    snapshot,
    parentUri,
    validation.normalized,
    documentMetadata,
    excludeUri,
    pendingNameResolver,
  );
  if (conflict) {
    const kind = conflict.type === "directory" ? "folder" : "file";
    return {
      ok: false,
      message: `A ${kind} named "${validation.normalized}" already exists here`,
    };
  }
  return { ok: true, normalized: validation.normalized };
}
