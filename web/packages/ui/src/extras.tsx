import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { highlight } from "./highlight.ts";
import { Icon } from "./icons.tsx";
import { Button, Dialog, IconButton } from "./primitives.tsx";

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(" ");

/**
 * The in-app replacement for `window.confirm`: says what will happen, names the consequence and
 * keeps the dialog open to show a failure (`error`) so the person can read it and decide.
 */
export function ConfirmDialog({
  title,
  confirmLabel,
  cancelLabel = "Cancel",
  danger = true,
  busy,
  error,
  onConfirm,
  onClose,
  children,
}: {
  title: string;
  /** Leave out to show only the close button (the dialog is reporting a result). */
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  busy?: boolean;
  /** Shown above the buttons as an alert; the dialog stays open. */
  error?: ReactNode;
  onConfirm?: () => void;
  onClose: () => void;
  children?: ReactNode;
}) {
  const bodyId = useId();
  return (
    <Dialog
      role="alertdialog"
      size="sm"
      title={title}
      describedBy={bodyId}
      className="confirm-dialog"
      onClose={busy ? () => {} : onClose}
      footer={
        <>
          <Button data-autofocus onClick={onClose} disabled={busy}>
            {cancelLabel}
          </Button>
          {confirmLabel && (
            <Button variant={danger ? "danger" : "primary"} loading={busy} onClick={onConfirm}>
              {confirmLabel}
            </Button>
          )}
        </>
      }
    >
      <div id={bodyId} className="confirm-body">
        {children}
        {error && (
          <div className="confirm-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </Dialog>
  );
}

/** "1–50 of 1,234" with previous, next and a page-size choice. */
export function Pager({
  offset,
  limit,
  total,
  onOffset,
  sizes = [25, 50, 100, 200],
  onSize,
  noun = "records",
  busy,
}: {
  offset: number;
  limit: number;
  total: number;
  onOffset: (offset: number) => void;
  sizes?: number[];
  onSize?: (size: number) => void;
  noun?: string;
  busy?: boolean;
}) {
  const id = useId();
  const nf = new Intl.NumberFormat("en-US");
  const from = total === 0 ? 0 : offset + 1;
  const to = Math.min(offset + limit, total);
  const last = Math.max(0, Math.floor((total - 1) / limit) * limit);
  const page = Math.floor(offset / limit) + 1;
  const pages = Math.max(1, Math.ceil(total / limit));
  return (
    <nav className="pager" aria-label={`Pages of ${noun}`} aria-busy={busy || undefined}>
      {onSize && (
        <label className="pager-size">
          <span>Rows per page</span>
          <select value={limit} onChange={(e) => onSize(Number(e.target.value))}>
            {sizes.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
      )}
      <span id={id} className="pager-range tabular" role="status" aria-live="polite">
        {total === 0 ? `No ${noun}` : `${nf.format(from)}–${nf.format(to)} of ${nf.format(total)}`}
      </span>
      <span className="pager-page hint tabular">
        Page {nf.format(page)} of {nf.format(pages)}
      </span>
      <div className="pager-btns">
        <IconButton
          icon="chevron-left"
          label="Previous page"
          size="sm"
          disabled={offset <= 0}
          onClick={() => onOffset(Math.max(0, offset - limit))}
        />
        <IconButton
          icon="chevron-right"
          label="Next page"
          size="sm"
          disabled={offset + limit >= total}
          onClick={() => onOffset(Math.min(last, offset + limit))}
        />
      </div>
    </nav>
  );
}

/** One big number with a label: a KPI tile. */
export function Stat({
  label,
  value,
  hint,
  tone,
}: {
  label: string;
  value: ReactNode;
  hint?: ReactNode;
  tone?: "neutral" | "accent" | "success" | "warning" | "danger";
}) {
  return (
    <div className={cx("stat", tone && tone !== "neutral" && `stat-${tone}`)}>
      <span className="stat-label">{label}</span>
      <strong className="stat-value tabular">{value}</strong>
      {hint && <span className="stat-hint">{hint}</span>}
    </div>
  );
}

/**
 * A block of code with a copy button. Highlighted from tokens (never from markup), so the text
 * is always rendered as text.
 */
export function CodeBlock({ code, lang = "" }: { code: string; lang?: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const lines = highlight(code, lang);
  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      // No clipboard permission: select the text so Ctrl+C works.
      const pre = root.current?.querySelector("pre");
      const sel = getSelection();
      if (sel && pre) {
        const r = document.createRange();
        r.selectNodeContents(pre);
        sel.removeAllRanges();
        sel.addRange(r);
      }
    }
    setCopied(true);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setCopied(false), 1800);
  }
  return (
    <div className="codeblock" data-lang={lang || undefined} ref={root}>
      <div className="codeblock-bar">
        <span className="codeblock-lang">{lang || "text"}</span>
        <button type="button" className="codeblock-copy" onClick={() => void copy()}>
          <Icon name={copied ? "check" : "copy"} size={14} />
          <span>{copied ? "Copied" : "Copy"}</span>
        </button>
      </div>
      <pre tabIndex={0} aria-label={`${lang || "Code"} example`}>
        <code>
          {lines.map((toks, i) => (
            <span key={i} className="codeblock-line">
              {toks.map((t, j) => (
                <span key={j} className={t.cls ? `tok-${t.cls}` : undefined}>
                  {t.text}
                </span>
              ))}
              {i < lines.length - 1 ? "\n" : ""}
            </span>
          ))}
        </code>
      </pre>
      <span className="sr-only" role="status" aria-live="polite">
        {copied ? "Copied to the clipboard" : ""}
      </span>
    </div>
  );
}
