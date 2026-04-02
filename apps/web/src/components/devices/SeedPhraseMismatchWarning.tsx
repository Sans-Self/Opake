import { WarningIcon } from "@phosphor-icons/react";
import { cn } from "@/lib/cn";
import { PageHeader } from "./PageHeader";

interface SeedPhraseMismatchWarningProps {
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
  readonly loading?: boolean;
  readonly error?: string | null;
}

export function SeedPhraseMismatchWarning({
  onConfirm,
  onCancel,
  loading,
  error,
}: SeedPhraseMismatchWarningProps) {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        icon={WarningIcon}
        iconClassName="text-warning"
        title="Key mismatch"
        description="The seed phrase you entered produces a different key than the one published on your account."
      />

      <div className="bg-base-200 max-w-md rounded-lg p-4 text-left text-sm">
        <p className="text-base-content/70">This usually means:</p>
        <ul className="text-base-content/60 mt-2 list-inside list-disc space-y-1">
          <li>The seed phrase is for a different account</li>
          <li>The account was set up with a random key (before seed phrases)</li>
        </ul>
        <p className="text-base-content/70 mt-3">
          If you continue, you won't be able to decrypt files encrypted with the old key.
        </p>
      </div>

      {error && (
        <p className="text-error text-sm" role="alert">
          {error}
        </p>
      )}

      <div className="flex gap-3">
        <button onClick={onCancel} className="btn btn-ghost btn-sm" disabled={loading}>
          Go back
        </button>
        <button
          onClick={onConfirm}
          disabled={loading}
          className={cn("btn btn-warning btn-sm", loading && "loading")}
        >
          {loading ? "Saving..." : "Use this phrase anyway"}
        </button>
      </div>
    </div>
  );
}
