// Toast notification renderer — fixed-position overlay with ARIA live region.

import { useEffect, useRef } from "react";
import {
  CheckCircleIcon,
  InfoIcon,
  WarningCircleIcon,
  WarningIcon,
  XIcon,
} from "@phosphor-icons/react";
import { useToastStore } from "@/stores/toast";

// ---------------------------------------------------------------------------
// Variant config
// ---------------------------------------------------------------------------

const VARIANT_STYLES = {
  success: { alertClass: "alert-success", Icon: CheckCircleIcon },
  error: { alertClass: "alert-error", Icon: WarningCircleIcon },
  warning: { alertClass: "alert-warning", Icon: WarningIcon },
  info: { alertClass: "alert-info", Icon: InfoIcon },
} as const;

// ---------------------------------------------------------------------------
// Toast item
// ---------------------------------------------------------------------------

interface ToastItemProps {
  readonly id: string;
  readonly variant: keyof typeof VARIANT_STYLES;
  readonly message: string;
  readonly duration: number;
}

function ToastItem({ id, variant, message, duration }: ToastItemProps) {
  const dismiss = useToastStore((s) => s.dismissToast);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(null);

  useEffect(() => {
    timerRef.current = setTimeout(() => dismiss(id), duration);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [id, duration, dismiss]);

  const { alertClass, Icon } = VARIANT_STYLES[variant];

  return (
    <div
      role="status"
      className={`alert ${alertClass} animate-toast-enter shadow-panel-md pointer-events-auto flex items-center gap-2 rounded-xl px-4 py-3`}
    >
      <Icon size={18} weight="fill" className="shrink-0" />
      <span className="text-ui flex-1">{message}</span>
      <button
        onClick={() => dismiss(id)}
        className="btn btn-ghost btn-xs btn-square rounded-md"
        aria-label="Dismiss notification"
      >
        <XIcon size={14} />
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Container
// ---------------------------------------------------------------------------

export function ToastContainer() {
  const toasts = useToastStore((s) => s.toasts);

  return (
    <div
      role="region"
      aria-label="Notifications"
      className="pointer-events-none fixed right-4 bottom-4 z-200 flex w-80 flex-col gap-2"
    >
      {/* Screen reader live region — visually hidden */}
      <div className="sr-only" aria-live="polite" aria-atomic="false">
        {toasts.map((t) => (
          <div key={t.id}>{t.message}</div>
        ))}
      </div>

      {/* Visible toasts */}
      {toasts.map((t) => (
        <ToastItem
          key={t.id}
          id={t.id}
          variant={t.variant}
          message={t.message}
          duration={t.duration}
        />
      ))}
    </div>
  );
}
