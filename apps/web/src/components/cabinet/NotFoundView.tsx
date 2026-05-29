// Empty-state view shown when a name-path URL doesn't resolve to a
// directory in the current snapshot.
//
// Triggered by FileView once the tree has loaded and a non-empty
// pathSegments fails the name-walk. The typical cause is a stale URL
// (renamed or moved by another writer) — rkey cascade no longer causes
// it. We surface what was attempted, why it might have failed, and offer
// affordances to recover (parent folder if any segment resolved, otherwise
// back to root).

import { Link } from "@tanstack/react-router";
import { FolderDashedIcon, ArrowUUpLeftIcon, HouseIcon } from "@phosphor-icons/react";

interface NotFoundViewProps {
  /** Whether the missing target is a directory or a file leaf. */
  readonly kind: "folder" | "file";
  /** The name of the missing segment (folder name or file name). */
  readonly missingName: string;
  /** Names of the directory segments that *did* resolve, in path order. */
  readonly resolvedDirSegments: readonly string[];
  /** The label shown for the workspace/cabinet root (e.g. "Your Cabinet"). */
  readonly rootLabel: string;
  /** The base URL for this context (e.g. "/cabinet/files"). */
  readonly basePath: string;
}

export function NotFoundView({
  kind,
  missingName,
  resolvedDirSegments,
  rootLabel,
  basePath,
}: NotFoundViewProps) {
  const parentLocationLabel =
    resolvedDirSegments.length === 0 ? rootLabel : resolvedDirSegments.join(" / ");
  const parentPath =
    resolvedDirSegments.length === 0 ? basePath : `${basePath}/${resolvedDirSegments.join("/")}`;
  // "Go to parent folder" only adds value when the parent isn't the
  // root — the "Back to {rootLabel}" link already covers that case.
  const showParentLink = resolvedDirSegments.length > 0;

  const heading = kind === "folder" ? "Folder not found" : "File not found";
  const reasons =
    kind === "folder"
      ? [
          "It may have been renamed or moved",
          "The link may be out of date",
          "It may have been deleted",
        ]
      : [
          "It may have been renamed",
          "The link may be out of date",
          "It may have been deleted from this folder",
        ];

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 px-6 py-16 text-center">
      <FolderDashedIcon size={56} weight="thin" className="text-text-faint" />

      <div className="max-w-md space-y-2">
        <h2 className="font-display text-base-content text-2xl">{heading}</h2>
        <p className="text-text-muted text-sm">
          We couldn&rsquo;t find{" "}
          <code className="bg-base-200 text-base-content rounded px-1.5 py-0.5 font-mono text-xs">
            {missingName}
          </code>{" "}
          in <span className="text-base-content">{parentLocationLabel}</span>.
        </p>
      </div>

      <ul className="text-text-faint max-w-md space-y-1 text-left text-sm">
        {reasons.map((r) => (
          <li key={r}>· {r}</li>
        ))}
      </ul>

      <div className="flex flex-wrap items-center justify-center gap-2">
        {showParentLink && (
          <Link to={parentPath} className="btn btn-primary btn-sm gap-1.5">
            <ArrowUUpLeftIcon size={14} />
            Go to parent
          </Link>
        )}
        <Link to={basePath} className="btn btn-ghost btn-sm gap-1.5">
          <HouseIcon size={14} />
          Back to {rootLabel}
        </Link>
      </div>
    </div>
  );
}
