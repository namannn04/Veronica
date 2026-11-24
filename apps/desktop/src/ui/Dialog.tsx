import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

import { IconButton } from "./Button";
import "./Dialog.css";

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 480,
  locked = false,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  width?: number;
  locked?: boolean;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const restore = useRef<Element | null>(null);

  useEffect(() => {
    if (!open) return;
    restore.current = document.activeElement;
    const frame = requestAnimationFrame(() => {
      const target = panel.current?.querySelector<HTMLElement>("[data-autofocus], input, textarea, select, button:not(.ui-dialog-close)");
      (target ?? panel.current)?.focus();
    });
    return () => {
      cancelAnimationFrame(frame);
      if (restore.current instanceof HTMLElement) restore.current.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!open || locked) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [locked, onClose, open]);

  if (!open) return null;

  return createPortal(
    <div className="ui-dialog-scrim" onMouseDown={locked ? undefined : onClose}>
      <div
        ref={panel}
        className="ui-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        style={{ width: `min(${width}px, calc(100vw - 32px))` }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="ui-dialog-head">
          <div className="ui-dialog-titles">
            <h2 id={titleId} className="ui-dialog-title">{title}</h2>
            {description && <p className="ui-dialog-description">{description}</p>}
          </div>
          {!locked && <IconButton className="ui-dialog-close" size="small" label="Close" icon={<X />} onClick={onClose} />}
        </div>
        {children && <div className="ui-dialog-body">{children}</div>}
        {footer && <div className="ui-dialog-footer">{footer}</div>}
      </div>
    </div>,
    document.body,
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  message,
  confirmLabel,
  destructive = false,
  busy = false,
  children,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: ReactNode;
  message?: ReactNode;
  confirmLabel: string;
  destructive?: boolean;
  busy?: boolean;
  children?: ReactNode;
}) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      description={message}
      width={420}
      locked={busy}
      footer={
        <>
          <button type="button" className="ui-button ui-button-secondary ui-button-regular" onClick={onClose} disabled={busy}>
            <span className="ui-button-label">Cancel</span>
          </button>
          <button
            type="button"
            data-autofocus
            className={`ui-button ${destructive ? "ui-button-destructive" : "ui-button-primary"} ui-button-regular`}
            onClick={onConfirm}
            disabled={busy}
            aria-busy={busy || undefined}
          >
            {busy && <span className="ui-button-spinner" aria-hidden="true" />}
            <span className="ui-button-label">{confirmLabel}</span>
          </button>
        </>
      }
    >
      {children}
    </Dialog>
  );
}
