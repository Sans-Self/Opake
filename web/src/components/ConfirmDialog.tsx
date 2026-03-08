import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";

const MODAL_TRANSITION_MS = 200;

export interface ConfirmDialogHandle {
  /** Show the dialog. `key` is passed to onConfirm; `label` is passed to the render function. */
  readonly show: (key: string, label: string) => void;
}

interface ConfirmDialogProps {
  readonly title: string;
  readonly icon?: ComponentType<{ readonly size: number; readonly className?: string }>;
  readonly iconClassName?: string;
  readonly iconBgClassName?: string;
  readonly children: (label: string) => ReactNode;
  readonly confirmLabel?: string;
  readonly confirmClassName?: string;
  readonly cancelLabel?: string;
  readonly onConfirm: (key: string) => void;
}

export const ConfirmDialog = forwardRef<ConfirmDialogHandle, ConfirmDialogProps>(
  function ConfirmDialog(
    {
      title,
      icon: Icon,
      iconClassName,
      iconBgClassName,
      children,
      confirmLabel = "Confirm",
      confirmClassName = "btn btn-primary btn-sm rounded-lg text-xs",
      cancelLabel = "Cancel",
      onConfirm,
    },
    ref,
  ) {
    const dialogRef = useRef<HTMLDialogElement>(null);
    const [pending, setPending] = useState<{ readonly key: string; readonly label: string } | null>(
      null,
    );

    const dismiss = useCallback(() => {
      dialogRef.current?.close();
      setTimeout(() => setPending(null), MODAL_TRANSITION_MS);
    }, []);

    const handleConfirm = useCallback(() => {
      if (pending) {
        onConfirm(pending.key);
      }
      dismiss();
    }, [pending, onConfirm, dismiss]);

    useImperativeHandle(ref, () => ({
      show: (key: string, label: string) => {
        setPending({ key, label });
        dialogRef.current?.showModal();
      },
    }));

    return (
      <dialog ref={dialogRef} className="modal" aria-label={title}>
        <div className="modal-box max-w-sm">
          <div className="flex flex-col items-center gap-3 text-center">
            {Icon && (
              <div
                className={`flex size-11 items-center justify-center rounded-full ${iconBgClassName ?? ""}`}
              >
                <Icon size={20} className={iconClassName} />
              </div>
            )}
            <h3 className="text-base-content text-sm font-semibold">{title}</h3>
            {pending && <div className="text-text-muted text-xs">{children(pending.label)}</div>}
          </div>
          <div className="modal-action justify-center gap-2">
            <button onClick={dismiss} className="btn btn-ghost btn-sm rounded-lg text-xs">
              {cancelLabel}
            </button>
            <button onClick={handleConfirm} className={confirmClassName}>
              {confirmLabel}
            </button>
          </div>
        </div>
        <form method="dialog" className="modal-backdrop">
          <button aria-label="Close">close</button>
        </form>
      </dialog>
    );
  },
);
