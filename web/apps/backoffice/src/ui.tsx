import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import type { FieldMeta } from "./api.ts";
import type { Tone } from "./data.tsx";
import { optionLabel } from "./format.ts";

/** Close a popover on outside click or Escape. */
export function useDismiss(ref: RefObject<HTMLElement | null>, open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) close();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        close();
      }
    };
    addEventListener("mousedown", down);
    addEventListener("keydown", key, true);
    return () => {
      removeEventListener("mousedown", down);
      removeEventListener("keydown", key, true);
    };
  }, [ref, open, close]);
}

export function Icon({ name, size = 16 }: { name: string; size?: number }) {
  const paths: Record<string, ReactNode> = {
    search: (
      <>
        <circle cx="11" cy="11" r="6.5" />
        <path d="m16 16 4 4" />
      </>
    ),
    caret: <path d="m6 9 6 6 6-6" />,
    caretRight: <path d="m9 6 6 6-6 6" />,
    plus: <path d="M12 5v14M5 12h14" />,
    close: <path d="m6 6 12 12M18 6 6 18" />,
    list: <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" />,
    kanban: (
      <>
        <rect x="3" y="4" width="5" height="16" rx="1.5" />
        <rect x="10" y="4" width="5" height="10" rx="1.5" />
        <rect x="17" y="4" width="4" height="13" rx="1.5" />
      </>
    ),
    pivot: (
      <>
        <rect x="3" y="4" width="18" height="16" rx="2" />
        <path d="M3 10h18M9 4v16" />
      </>
    ),
    graph: (
      <>
        <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />
      </>
    ),
    calendar: (
      <>
        <rect x="3" y="5" width="18" height="16" rx="2" />
        <path d="M3 10h18M8 3v4M16 3v4" />
      </>
    ),
    star: <path d="m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z" />,
    columns: (
      <>
        <rect x="3" y="4" width="18" height="16" rx="2" />
        <path d="M9 4v16M15 4v16" />
      </>
    ),
    download: <path d="M12 4v11m-5-4 5 5 5-5M5 20h14" />,
    external: (
      <path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
    ),
    menu: <path d="M4 6h16M4 12h16M4 18h16" />,
    check: <path d="m5 12 5 5 9-10" />,
    shield: <path d="M12 3 4 6v6c0 4.5 3.2 7.8 8 9 4.8-1.2 8-4.5 8-9V6z" />,
  };
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="ico"
    >
      {paths[name]}
    </svg>
  );
}

const OK = new Set([
  "done",
  "won",
  "posted",
  "confirmed",
  "closed",
  "available",
  "running",
  "resolved",
  "active",
  "paid",
]);
const BAD = new Set(["lost", "cancelled", "canceled", "failed", "rejected"]);
const WARN = new Set(["doing", "in_service", "maintenance", "in_progress", "qualified", "pending"]);
const NEUTRAL = new Set(["draft", "new", "open", "todo", "planned", "none", ""]);

/** Status-like words keep a meaning-carrying colour, any other option takes a palette colour. */
export function toneClass(f: FieldMeta | undefined, value: string): string {
  const v = value.toLowerCase();
  if (OK.has(v)) return "t-ok";
  if (BAD.has(v)) return "t-bad";
  if (WARN.has(v)) return "t-warn";
  if (NEUTRAL.has(v)) return "t-neutral";
  const i = f?.options?.indexOf(value) ?? 0;
  return `t-${(Math.max(i, 0) % 6) + 1}`;
}

export function Badge({ field, value }: { field?: FieldMeta; value: string }) {
  return <span className={`badge ${toneClass(field, value)}`}>{optionLabel(value)}</span>;
}

export function Skeleton({ rows = 6, cols = 5 }: { rows?: number; cols?: number }) {
  return (
    <div className="skeleton" role="status" aria-label="Loading">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="sk-row">
          {Array.from({ length: cols }, (_, j) => (
            <span key={j} style={{ width: `${40 + ((i * 7 + j * 13) % 45)}%` }} />
          ))}
        </div>
      ))}
    </div>
  );
}

export function EmptyState(props: {
  title: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondary?: { label: string; onClick: () => void };
}) {
  return (
    <div className="empty">
      <div className="empty-art" aria-hidden="true">
        <svg
          viewBox="0 0 48 48"
          width="34"
          height="34"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="7" y="10" width="34" height="28" rx="5" />
          <path d="M7 20h34M16 29h8" />
        </svg>
      </div>
      <h3>{props.title}</h3>
      {props.hint && <p className="hint">{props.hint}</p>}
      <div className="actionrow center">
        {props.action && (
          <button className="btn primary" onClick={props.action.onClick}>
            {props.action.label}
          </button>
        )}
        {props.secondary && (
          <button className="btn" onClick={props.secondary.onClick}>
            {props.secondary.label}
          </button>
        )}
      </div>
    </div>
  );
}

export type ToastItem = { id: number; text: string; tone: Tone };

/** One polite live region, so screen readers hear every toast. */
export function Toasts({
  items,
  onDismiss,
}: {
  items: ToastItem[];
  onDismiss: (id: number) => void;
}) {
  return (
    <div className="toasts" role="status" aria-live="polite" aria-atomic="false">
      {items.map((t) => (
        <div key={t.id} className={`toast panel ${t.tone}`}>
          <span>{t.text}</span>
          <button className="icon-btn" onClick={() => onDismiss(t.id)} aria-label="Dismiss">
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}

export type ConfirmRequest = {
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  resolve: (ok: boolean) => void;
};

export function ConfirmDialog({ req }: { req: ConfirmRequest }) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    cancel.current?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        req.resolve(false);
      }
    };
    addEventListener("keydown", key, true);
    return () => removeEventListener("keydown", key, true);
  }, [req]);
  return (
    <div className="overlay" onMouseDown={() => req.resolve(false)}>
      <div
        className="confirm panel"
        role="alertdialog"
        aria-modal="true"
        aria-label="Confirm"
        aria-describedby="confirm-msg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <p id="confirm-msg">{req.message}</p>
        <div className="actionrow end">
          <button ref={cancel} className="btn" onClick={() => req.resolve(false)}>
            {req.cancelLabel}
          </button>
          <button className="btn danger" onClick={() => req.resolve(true)}>
            {req.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** A button that opens a floating panel under it. */
export function Dropdown(props: {
  label: ReactNode;
  ariaLabel?: string;
  className?: string;
  align?: "start" | "end";
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(false);
    btn.current?.focus();
  };
  useDismiss(ref, open, () => setOpen(false));
  return (
    <div className="dropdown" ref={ref}>
      <button
        ref={btn}
        className={props.className ?? "btn"}
        aria-haspopup="true"
        aria-expanded={open}
        aria-label={props.ariaLabel}
        onClick={() => setOpen(!open)}
      >
        {props.label}
      </button>
      {open && (
        <div className={`dd-panel panel ${props.align === "end" ? "end" : ""}`}>
          {props.children(close)}
        </div>
      )}
    </div>
  );
}
