import { useCallback, useEffect, useRef, useState } from "react";

import { ipc, listenEvent } from "../lib/ipc";
import { Button, Dialog, Notice, type Tone } from "../ui";
import { type InterfaceRequest, interfaceRequest } from "./bridge";
import "./InterfaceHost.css";

const TOAST_MILLISECONDS = 5000;
const MAX_TOASTS = 4;

const TOAST_TONES: Record<
  Extract<InterfaceRequest, { action: "toast" }>["kind"],
  Exclude<Tone, "accent">
> = {
  info: "neutral",
  success: "good",
  warning: "warning",
  error: "critical",
};

type Toast = Extract<InterfaceRequest, { action: "toast" }> & { key: number };
type Confirmation = Extract<InterfaceRequest, { action: "confirm" }>;
export type NavRequest = Extract<InterfaceRequest, { action: "open" | "badge" | "title" }>;

export function InterfaceHost({
  titleOf,
  onNav,
}: {
  titleOf: (extension: string) => string;
  onNav: (request: NavRequest) => void;
}) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [confirmations, setConfirmations] = useState<Confirmation[]>([]);
  const nextToast = useRef(0);
  const navigate = useRef(onNav);
  navigate.current = onNav;

  useEffect(() => {
    const timers = new Set<ReturnType<typeof setTimeout>>();
    const stop = listenEvent("extension-interface", (event) => {
      const request = interfaceRequest(event.payload);
      if (request === null) {
        return;
      }
      if (request.action === "toast") {
        const key = nextToast.current++;
        setToasts((current) => [...current, { ...request, key }].slice(-MAX_TOASTS));
        const timer = setTimeout(() => {
          timers.delete(timer);
          setToasts((current) => current.filter((toast) => toast.key !== key));
        }, TOAST_MILLISECONDS);
        timers.add(timer);
      } else if (request.action === "confirm") {
        setConfirmations((current) => [...current, request]);
      } else {
        navigate.current(request);
      }
    });
    return () => {
      void stop.then((unlisten) => unlisten());
      for (const timer of timers) {
        clearTimeout(timer);
      }
    };
  }, []);

  const current = confirmations[0];
  const settle = useCallback((request: number, accepted: boolean) => {
    setConfirmations((queue) => queue.filter((item) => item.request !== request));
    void ipc.extConfirmAnswer(request, accepted).catch(() => undefined);
  }, []);
  const decline = useCallback(() => {
    if (current !== undefined) {
      settle(current.request, false);
    }
  }, [current, settle]);

  return (
    <>
      {toasts.length > 0 && (
        <div className="extension-toasts" aria-live="polite">
          {toasts.map((toast) => (
            <Notice
              key={toast.key}
              tone={TOAST_TONES[toast.kind]}
              title={titleOf(toast.extension)}
              compact
            >
              {toast.message}
            </Notice>
          ))}
        </div>
      )}
      {current !== undefined && (
        <Dialog
          open
          onClose={decline}
          title={current.title}
          description={`${titleOf(current.extension)} is asking`}
          width={420}
          footer={
            <>
              <Button onClick={decline}>{current.cancelLabel ?? "Cancel"}</Button>
              <Button
                data-autofocus
                variant={current.destructive ? "destructive" : "primary"}
                onClick={() => settle(current.request, true)}
              >
                {current.confirmLabel ?? "Continue"}
              </Button>
            </>
          }
        >
          {current.body !== null && <p className="extension-confirm-body">{current.body}</p>}
        </Dialog>
      )}
    </>
  );
}
