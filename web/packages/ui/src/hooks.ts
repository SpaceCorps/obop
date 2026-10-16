import { useEffect, useRef, type RefObject } from "react";

/** Close a popover on outside click or Escape. */
export function useDismiss(ref: RefObject<HTMLElement | null>, open: boolean, close: () => void) {
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      // composedPath is taken at dispatch, so an item that unmounts itself on click still counts as inside
      if (ref.current && !e.composedPath().includes(ref.current)) close();
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

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Keep keyboard focus inside `ref` while `active`, focus the first control (or the element marked
 * `data-autofocus`) on open and hand focus back to whatever had it before on close.
 */
export function useFocusTrap(ref: RefObject<HTMLElement | null>, active: boolean) {
  const returnTo = useRef<Element | null>(null);
  useEffect(() => {
    if (!active) return;
    const root = ref.current;
    if (!root) return;
    returnTo.current = document.activeElement;
    const items = () => [...root.querySelectorAll<HTMLElement>(FOCUSABLE)];
    (root.querySelector<HTMLElement>("[data-autofocus]") ?? items()[0] ?? root).focus();
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const list = items();
      if (list.length === 0) return e.preventDefault();
      const first = list[0]!;
      const last = list[list.length - 1]!;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    root.addEventListener("keydown", key);
    return () => {
      root.removeEventListener("keydown", key);
      (returnTo.current as HTMLElement | null)?.focus?.();
    };
  }, [ref, active]);
}

/** Arrow-key roving between the enabled buttons of a group (tabs, radio groups, menus). */
export function rovingKeys(
  e: React.KeyboardEvent<HTMLElement>,
  selector: string,
  orientation: "horizontal" | "vertical" = "horizontal",
  /** Selection follows focus (tabs, radios); menus only move focus. */
  activate = true,
) {
  const next = orientation === "horizontal" ? "ArrowRight" : "ArrowDown";
  const prev = orientation === "horizontal" ? "ArrowLeft" : "ArrowUp";
  if (![next, prev, "Home", "End"].includes(e.key)) return false;
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>(selector)].filter(
    (i) => !i.hasAttribute("disabled"),
  );
  if (!items.length) return false;
  const at = items.indexOf(document.activeElement as HTMLElement);
  const rtl = getComputedStyle(e.currentTarget).direction === "rtl" && orientation === "horizontal";
  const step = e.key === next ? (rtl ? -1 : 1) : e.key === prev ? (rtl ? 1 : -1) : 0;
  const to =
    e.key === "Home"
      ? 0
      : e.key === "End"
        ? items.length - 1
        : (Math.max(at, 0) + step + items.length) % items.length;
  e.preventDefault();
  items[to]!.focus();
  if (activate) items[to]!.click();
  return true;
}
