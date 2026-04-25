// Toast notification store — transient feedback for user-facing operations.

import { create } from "zustand";
import { immer } from "zustand/middleware/immer";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ToastVariant = "success" | "error" | "warning" | "info";

interface Toast {
  readonly id: string;
  readonly variant: ToastVariant;
  readonly message: string;
  readonly duration: number;
}

interface ToastState {
  toasts: Toast[];
  readonly addToast: (variant: ToastVariant, message: string, duration?: number) => string;
  readonly dismissToast: (id: string) => void;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_DURATIONS: Readonly<Record<ToastVariant, number>> = {
  success: 4000,
  error: 8000,
  warning: 6000,
  info: 5000,
};

const MAX_VISIBLE = 5;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useToastStore = create<ToastState>()(
  immer((set) => ({
    toasts: [],

    addToast: (variant, message, duration) => {
      const id = crypto.randomUUID();
      const resolvedDuration = duration ?? DEFAULT_DURATIONS[variant];

      set((draft) => {
        // Evict oldest when at capacity
        if (draft.toasts.length >= MAX_VISIBLE) {
          draft.toasts.shift();
        }
        draft.toasts.push({ id, variant, message, duration: resolvedDuration });
      });

      return id;
    },

    dismissToast: (id) => {
      set((draft) => {
        draft.toasts = draft.toasts.filter((t) => t.id !== id);
      });
    },
  })),
);

// ---------------------------------------------------------------------------
// Convenience functions — callable outside React components
// ---------------------------------------------------------------------------

export function toastSuccess(message: string, duration?: number): string {
  return useToastStore.getState().addToast("success", message, duration);
}

export function toastError(message: string, duration?: number): string {
  return useToastStore.getState().addToast("error", message, duration);
}

export function toastWarning(message: string, duration?: number): string {
  return useToastStore.getState().addToast("warning", message, duration);
}

export function toastInfo(message: string, duration?: number): string {
  return useToastStore.getState().addToast("info", message, duration);
}
