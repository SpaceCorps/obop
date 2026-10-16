import {
  cloneElement,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactElement,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { createPortal } from "react-dom";
import { rovingKeys, useDismiss, useFocusTrap } from "./hooks.ts";
import { Icon } from "./icons.tsx";
import type { IconName } from "./icon-data.ts";
import { t } from "./i18n/store.ts";

const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(" ");

/* ================= Buttons ================= */

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  /** Shows a spinner, keeps the label and blocks clicks. */
  loading?: boolean;
  icon?: IconName;
  iconEnd?: IconName;
};

export function Button({
  variant = "secondary",
  size = "md",
  loading,
  icon,
  iconEnd,
  className,
  children,
  disabled,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        "btn",
        variant !== "secondary" && variant,
        size !== "md" && (size === "sm" ? "small" : "large"),
        loading && "is-loading",
        className,
      )}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...rest}
    >
      {loading ? (
        <Spinner size={14} />
      ) : (
        icon && <Icon name={icon} size={size === "lg" ? 18 : 16} />
      )}
      {children}
      {iconEnd && <Icon name={iconEnd} size={size === "lg" ? 18 : 16} />}
    </button>
  );
}

export type IconButtonProps = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label"> & {
  icon: IconName;
  /** Required: an icon alone has no name. Also shown as the tooltip. */
  label: string;
  size?: "sm" | "md";
  iconSize?: number;
  tooltip?: boolean;
};

export function IconButton({
  icon,
  label,
  size = "md",
  iconSize,
  tooltip = true,
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  const btn = (
    <button
      type={type}
      className={cx("icon-btn", size === "sm" && "small", className)}
      aria-label={label}
      {...rest}
    >
      <Icon name={icon} size={iconSize ?? (size === "sm" ? 16 : 18)} />
    </button>
  );
  return tooltip && !rest.disabled ? <Tooltip label={label}>{btn}</Tooltip> : btn;
}

/* ================= Form controls ================= */

type FieldProps = {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  required?: boolean;
};

function useFieldIds(id: string | undefined, hint: ReactNode, error: ReactNode) {
  const auto = useId();
  const base = id ?? auto;
  const describedBy = [hint ? `${base}-hint` : "", error ? `${base}-err` : ""]
    .filter(Boolean)
    .join(" ");
  return { base, describedBy: describedBy || undefined };
}

function FieldShell({
  base,
  label,
  hint,
  error,
  required,
  className,
  children,
}: FieldProps & { base: string; className?: string; children: ReactNode }) {
  return (
    <div className={cx("field", !!error && "has-error", className)}>
      {label && (
        <label className="flabel" htmlFor={base}>
          {label}
          {required && (
            <span className="req" aria-hidden="true">
              *
            </span>
          )}
        </label>
      )}
      {children}
      {hint && (
        <p className="hint" id={`${base}-hint`}>
          {hint}
        </p>
      )}
      {error && (
        <p className="error" id={`${base}-err`}>
          <Icon name="warning" size={14} /> {error}
        </p>
      )}
    </div>
  );
}

export type InputProps = InputHTMLAttributes<HTMLInputElement> & FieldProps;

export function Input({ label, hint, error, id, required, className, ...rest }: InputProps) {
  const { base, describedBy } = useFieldIds(id, hint, error);
  return (
    <FieldShell {...{ base, label, hint, error, required, className }}>
      <input
        id={base}
        required={required}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        {...rest}
      />
    </FieldShell>
  );
}

export type SelectProps = SelectHTMLAttributes<HTMLSelectElement> & FieldProps;

export function Select({
  label,
  hint,
  error,
  id,
  required,
  className,
  children,
  ...rest
}: SelectProps) {
  const { base, describedBy } = useFieldIds(id, hint, error);
  return (
    <FieldShell {...{ base, label, hint, error, required, className }}>
      <span className="select-wrap">
        <select
          id={base}
          required={required}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          {...rest}
        >
          {children}
        </select>
        <Icon name="chevron-down" size={16} className="select-caret" />
      </span>
    </FieldShell>
  );
}

export type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & FieldProps;

export function Textarea({ label, hint, error, id, required, className, ...rest }: TextareaProps) {
  const { base, describedBy } = useFieldIds(id, hint, error);
  return (
    <FieldShell {...{ base, label, hint, error, required, className }}>
      <textarea
        id={base}
        required={required}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        {...rest}
      />
    </FieldShell>
  );
}

export type CheckboxProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & {
  label: ReactNode;
  indeterminate?: boolean;
  hint?: ReactNode;
};

export function Checkbox({ label, indeterminate, hint, className, ...rest }: CheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = !!indeterminate;
  }, [indeterminate]);
  return (
    <label className={cx("check-row", className)}>
      <input ref={ref} type="checkbox" className="checkbox" {...rest} />
      <span>
        {label}
        {hint && <span className="hint block">{hint}</span>}
      </span>
    </label>
  );
}

export type SwitchProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "role"> & {
  label: ReactNode;
};

export function Switch({ label, className, ...rest }: SwitchProps) {
  return (
    <label className={cx("switch", className)}>
      <input type="checkbox" role="switch" {...rest} />
      <span className="track" aria-hidden="true" />
      <span>{label}</span>
    </label>
  );
}

/* ================= Status and labels ================= */

export type Tone = "neutral" | "accent" | "success" | "warning" | "danger" | "info";
const TONE_CLASS: Record<Tone, string> = {
  neutral: "t-neutral",
  accent: "t-accent",
  success: "t-ok",
  warning: "t-warn",
  danger: "t-bad",
  info: "t-info",
};

export function Badge({
  tone = "neutral",
  dot = true,
  className,
  children,
  ...rest
}: HTMLAttributes<HTMLSpanElement> & { tone?: Tone | `t-${string}`; dot?: boolean }) {
  const t = tone.startsWith("t-") ? tone : TONE_CLASS[tone as Tone];
  return (
    <span className={cx("badge", t, className)} {...rest}>
      {dot && <i className="badge-dot" aria-hidden="true" />}
      {children}
    </span>
  );
}

export function Chip({
  children,
  onRemove,
  removeLabel,
  tone,
  className,
  ...rest
}: HTMLAttributes<HTMLSpanElement> & {
  onRemove?: () => void;
  removeLabel?: string;
  tone?: "accent" | "neutral";
}) {
  return (
    <span className={cx("chip", tone === "accent" && "accent", className)} {...rest}>
      <span className="chip-text">{children}</span>
      {onRemove && (
        <button
          type="button"
          className="chip-x"
          onClick={onRemove}
          aria-label={removeLabel ?? t("Remove")}
        >
          <Icon name="close" size={12} />
        </button>
      )}
    </span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

export function Avatar({ name, src, size = 32 }: { name: string; src?: string; size?: number }) {
  const initials =
    name
      .split(/[\s@._-]+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((w) => w[0]!.toUpperCase())
      .join("") || "?";
  return (
    <span
      className="avatar"
      style={{ width: size, height: size, fontSize: size * 0.4 }}
      aria-hidden="true"
    >
      {src ? <img src={src} alt="" /> : initials}
    </span>
  );
}

export function Spinner({ size = 18, label }: { size?: number; label?: string }) {
  return (
    <span
      className="spinner"
      style={{ width: size, height: size }}
      role={label ? "status" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    />
  );
}

export function Notice({
  tone = "info",
  title,
  children,
  action,
}: {
  tone?: "info" | "success" | "warning" | "danger";
  title?: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
}) {
  const icon: IconName = tone === "success" ? "check-circle" : tone === "info" ? "info" : "warning";
  return (
    <div className={cx("notice", tone)} role={tone === "danger" ? "alert" : "status"}>
      <Icon name={icon} size={18} />
      <div className="notice-body">
        {title && <strong>{title}</strong>}
        {children && <p>{children}</p>}
      </div>
      {action}
    </div>
  );
}

/* ================= Surfaces ================= */

export function Panel({
  as: As = "div",
  className,
  ...rest
}: HTMLAttributes<HTMLElement> & {
  as?: "div" | "section" | "aside" | "article" | "header" | "nav";
}) {
  return <As className={cx("panel", className)} {...rest} />;
}

export function Card({
  as: As = "div",
  interactive,
  className,
  ...rest
}: HTMLAttributes<HTMLElement> & {
  as?: "div" | "section" | "article" | "li";
  interactive?: boolean;
}) {
  return <As className={cx("ui-card", interactive && "interactive", className)} {...rest} />;
}

export function Table({
  caption,
  dense,
  className,
  children,
  ...rest
}: HTMLAttributes<HTMLTableElement> & { caption?: string; dense?: boolean }) {
  return (
    <div className="table-wrap">
      <table className={cx("table", dense && "dense", className)} {...rest}>
        {caption && <caption className="sr-only">{caption}</caption>}
        {children}
      </table>
    </div>
  );
}

export function Skeleton({ rows = 6, cols = 5 }: { rows?: number; cols?: number }) {
  return (
    <div className="skeleton" role="status" aria-label={t("Loading")}>
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
  icon?: IconName;
  action?: { label: string; onClick: () => void };
  secondary?: { label: string; onClick: () => void };
}) {
  return (
    <div className="empty">
      <div className="empty-art" aria-hidden="true">
        <Icon name={props.icon ?? "list"} size={28} />
      </div>
      <h3>{props.title}</h3>
      {props.hint && <p className="hint">{props.hint}</p>}
      {(props.action || props.secondary) && (
        <div className="actionrow center">
          {props.action && (
            <Button variant="primary" onClick={props.action.onClick}>
              {props.action.label}
            </Button>
          )}
          {props.secondary && (
            <Button onClick={props.secondary.onClick}>{props.secondary.label}</Button>
          )}
        </div>
      )}
    </div>
  );
}

/* ================= Tabs and segmented controls ================= */

export type TabItem<T extends string = string> = {
  id: T;
  label: string;
  icon?: IconName;
  count?: number;
};

export const tabId = (prefix: string, id: string) => `${prefix}-tab-${id}`;
export const panelId = (prefix: string) => `${prefix}-panel`;

/** An underlined tab list. The caller renders the panel with `panelId(prefix)`, role="tabpanel". */
export function Tabs<T extends string>({
  label,
  tabs,
  value,
  onChange,
  idPrefix,
}: {
  label: string;
  tabs: TabItem<T>[];
  value: T;
  onChange: (v: T) => void;
  idPrefix: string;
}) {
  return (
    <div
      className="tabs"
      role="tablist"
      aria-label={label}
      onKeyDown={(e) => rovingKeys(e, '[role="tab"]')}
    >
      {tabs.map((t) => (
        <button
          key={t.id}
          id={tabId(idPrefix, t.id)}
          type="button"
          role="tab"
          aria-selected={value === t.id}
          aria-controls={panelId(idPrefix)}
          tabIndex={value === t.id ? 0 : -1}
          onClick={() => onChange(t.id)}
        >
          {t.icon && <Icon name={t.icon} size={16} />}
          {t.label}
          {t.count !== undefined && <span className="tab-count">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}

export type SegmentedOption<T extends string = string> = {
  value: T;
  label: string;
  icon?: IconName;
};

/**
 * A pill group. `radio` (default) is one choice out of several (role radiogroup); `toggle` is a
 * set of pressed buttons, used for the view switcher.
 */
export function Segmented<T extends string>({
  label,
  options,
  value,
  onChange,
  mode = "radio",
  iconOnlyNarrow,
}: {
  label: string;
  options: SegmentedOption<T>[];
  value: T;
  onChange: (v: T) => void;
  mode?: "radio" | "toggle";
  /** Hide the text labels on narrow screens (the icon and the aria-label remain). */
  iconOnlyNarrow?: boolean;
}) {
  const radio = mode === "radio";
  return (
    <div
      className="switcher"
      role={radio ? "radiogroup" : "group"}
      aria-label={label}
      onKeyDown={radio ? (e) => rovingKeys(e, '[role="radio"]') : undefined}
    >
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role={radio ? "radio" : undefined}
          aria-checked={radio ? value === o.value : undefined}
          aria-pressed={radio ? undefined : value === o.value}
          aria-label={iconOnlyNarrow ? o.label : undefined}
          tabIndex={radio ? (value === o.value ? 0 : -1) : undefined}
          title={iconOnlyNarrow ? o.label : undefined}
          onClick={() => onChange(o.value)}
        >
          {o.icon && <Icon name={o.icon} size={15} />}
          <span className={iconOnlyNarrow ? "vlabel" : undefined}>{o.label}</span>
        </button>
      ))}
    </div>
  );
}

/* ================= Popover, Menu, Tooltip ================= */

/** A button that opens a floating panel under it. */
export function Popover(props: {
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
        type="button"
        className={props.className ?? "btn"}
        aria-haspopup="true"
        aria-expanded={open}
        aria-label={props.ariaLabel}
        onClick={() => setOpen(!open)}
      >
        {props.label}
      </button>
      {open && (
        <div className={cx("dd-panel panel", props.align === "end" && "end")}>
          {props.children(close)}
        </div>
      )}
    </div>
  );
}

export type MenuItem =
  | "separator"
  | { heading: string }
  | {
      label: string;
      onSelect: () => void;
      icon?: IconName;
      danger?: boolean;
      disabled?: boolean;
      hint?: string;
      /** A second line under the label, for example why the item is disabled. Also the tooltip. */
      description?: string;
    };

/** An action menu: arrow keys move, Enter or Space picks, Escape closes and returns focus. */
export function Menu({
  trigger,
  ariaLabel,
  items,
  align = "start",
  triggerClassName,
}: {
  trigger: ReactNode;
  ariaLabel?: string;
  items: MenuItem[];
  align?: "start" | "end";
  triggerClassName?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const id = useId();
  const close = useCallback((refocus = true) => {
    setOpen(false);
    if (refocus) btn.current?.focus();
  }, []);
  useDismiss(ref, open, () => close(true));
  useEffect(() => {
    if (open)
      list.current?.querySelector<HTMLElement>('[role="menuitem"]:not([disabled])')?.focus();
  }, [open]);
  return (
    <div className="dropdown" ref={ref}>
      <button
        ref={btn}
        type="button"
        className={triggerClassName ?? "btn"}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={ariaLabel}
        onClick={() => setOpen(!open)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        {trigger}
      </button>
      {open && (
        <div
          id={id}
          ref={list}
          role="menu"
          aria-label={ariaLabel}
          className={cx("dd-panel panel menu-list", align === "end" && "end")}
          onKeyDown={(e) => {
            rovingKeys(e, '[role="menuitem"]', "vertical", false);
            if (e.key === "Tab") close(false);
          }}
        >
          {items.map((it, i) =>
            it === "separator" ? (
              <hr key={i} className="menu-sep" />
            ) : "heading" in it ? (
              <div key={`h${i}`} className="menu-head" role="presentation">
                {it.heading}
              </div>
            ) : (
              <button
                key={it.label}
                type="button"
                role="menuitem"
                disabled={it.disabled}
                title={it.description}
                className={cx("menu-item", it.danger && "danger", it.description && "described")}
                onClick={() => {
                  close();
                  it.onSelect();
                }}
              >
                {it.icon && <Icon name={it.icon} size={16} />}
                <span>
                  {it.label}
                  {it.description && <small className="menu-why">{it.description}</small>}
                </span>
                {it.hint && <Kbd>{it.hint}</Kbd>}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}

/** A short label for an icon or truncated control. Shown on hover and keyboard focus, Escape hides it. */
export function Tooltip({
  label,
  children,
  side = "bottom",
  align = "center",
}: {
  label: string;
  children: ReactElement<{ "aria-describedby"?: string }>;
  side?: "top" | "bottom";
  /** `start` or `end` lines the tip up with that edge of its target, for controls near an edge. */
  align?: "center" | "start" | "end";
}) {
  const [show, setShow] = useState(false);
  const id = useId();
  const timer = useRef<number | undefined>(undefined);
  const on = () => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setShow(true), 350);
  };
  const off = () => {
    window.clearTimeout(timer.current);
    setShow(false);
  };
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <span
      className="tip"
      onMouseEnter={on}
      onMouseLeave={off}
      onFocus={() => setShow(true)}
      onBlur={off}
      onKeyDown={(e) => e.key === "Escape" && off()}
    >
      {cloneElement(children, show ? { "aria-describedby": id } : {})}
      {show && (
        <span role="tooltip" id={id} className={cx("tip-body", side, align !== "center" && align)}>
          {label}
        </span>
      )}
    </span>
  );
}

/* ================= Dialog, Sheet, Toast ================= */

export type DialogProps = {
  open?: boolean;
  onClose: () => void;
  title: string;
  /** Visually hide the heading but keep it as the accessible name. */
  hideTitle?: boolean;
  children?: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg";
  /** `alertdialog` for a question that needs an answer (confirm, discard). */
  role?: "dialog" | "alertdialog";
  /** `sheet` slides in from the end edge (the bottom on phones). */
  variant?: "dialog" | "sheet";
  /** No header or close button: the title stays as the accessible name (launcher, command palette). */
  bare?: boolean;
  /** Where the dialog sits: centred (default) or near the top, like a command palette. */
  placement?: "center" | "top";
  className?: string;
  describedBy?: string;
};

/** A modal: focus trap, Escape and backdrop close, focus returns to the opener. */
export function Dialog({
  open = true,
  onClose,
  title,
  hideTitle,
  children,
  footer,
  size = "md",
  role = "dialog",
  variant = "dialog",
  bare,
  placement = "center",
  className,
  describedBy,
}: DialogProps) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  useFocusTrap(ref, open);
  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    addEventListener("keydown", key, true);
    return () => removeEventListener("keydown", key, true);
  }, [open, onClose]);
  if (!open) return null;
  return createPortal(
    <div
      className={cx(
        "overlay",
        variant === "sheet" && "sheet-overlay",
        placement === "top" && "top",
      )}
      onMouseDown={onClose}
    >
      <div
        ref={ref}
        className={cx("dialog panel", `size-${size}`, variant === "sheet" && "sheet", className)}
        role={role}
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={describedBy}
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {bare ? (
          <h2 id={titleId} className="sr-only">
            {title}
          </h2>
        ) : (
          <header className="dialog-head">
            <h2 id={titleId} className={hideTitle ? "sr-only" : undefined}>
              {title}
            </h2>
            <IconButton icon="close" label={t("Close")} size="sm" onClick={onClose} />
          </header>
        )}
        <div className="dialog-body">{children}</div>
        {footer && <footer className="dialog-foot">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}

export type ToastTone = "ok" | "error" | "info";
export type ToastItem = { id: number; text: string; tone: ToastTone };

/** One polite live region, so screen readers hear every toast. */
export function ToastRegion({
  items,
  onDismiss,
}: {
  items: ToastItem[];
  onDismiss: (id: number) => void;
}) {
  return (
    <div className="toasts" role="status" aria-live="polite" aria-atomic="false">
      {items.map((t) => (
        <Toast key={t.id} tone={t.tone} onDismiss={() => onDismiss(t.id)}>
          {t.text}
        </Toast>
      ))}
    </div>
  );
}

export function Toast({
  tone = "ok",
  children,
  onDismiss,
}: {
  tone?: ToastTone;
  children: ReactNode;
  onDismiss?: () => void;
}) {
  const icon: IconName = tone === "ok" ? "check-circle" : tone === "error" ? "warning" : "info";
  return (
    <div className={cx("toast panel", tone)}>
      <Icon name={icon} size={18} className="toast-icon" />
      <span className="toast-text">{children}</span>
      {onDismiss && (
        <button
          type="button"
          className="icon-btn small"
          onClick={onDismiss}
          aria-label={t("Dismiss")}
        >
          <Icon name="close" size={14} />
        </button>
      )}
    </div>
  );
}
