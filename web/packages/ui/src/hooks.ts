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
  // The opener is read while the dialog renders: by the time the effect runs, a field of the dialog
  // with `autoFocus` has already taken focus, and that field is not what focus goes back to (R16).
  const opener = useRef<Element | null>(null);
  const wasActive = useRef(false);
  if (active && !wasActive.current && typeof document !== "undefined")
    opener.current = document.activeElement;
  wasActive.current = active;
  useEffect(() => {
    if (!active) return;
    const root = ref.current;
    if (!root) return;
    const now = document.activeElement;
    const inside = !!now && root.contains(now);
    returnTo.current = inside ? opener.current : now;
    const items = () => [...root.querySelectorAll<HTMLElement>(FOCUSABLE)];
    // A field that took focus itself keeps it.
    if (!inside)
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
      giveFocusBack(returnTo.current as HTMLElement | null);
    };
  }, [ref, active]);
}

/**
 * Focus goes back to what opened a dialog. The opener may be disabled while the dialog's work runs,
 * or drawn again as a new element (R16): then it is tried again once the page has drawn, and an
 * element of the same kind with the same name stands in for one that is gone.
 */
export function giveFocusBack(opener: HTMLElement | null) {
  if (!opener) return;
  const name = (e: Element) => e.getAttribute("aria-label") ?? e.textContent?.trim() ?? "";
  const tag = opener.tagName;
  const label = name(opener);
  const back = () => {
    const el = opener.isConnected
      ? opener
      : [...document.querySelectorAll<HTMLElement>(tag)].find(
          (e) => name(e) === label && !e.closest("[hidden], [inert]"),
        );
    el?.focus?.({ preventScroll: true });
    return !!el && document.activeElement === el;
  };
  if (back()) return;
  // Only when nothing else took the focus meanwhile (another dialog, a field the work opened).
  const later = () => {
    const a = document.activeElement;
    if (!a || a === document.body) back();
  };
  requestAnimationFrame(later);
  setTimeout(later, 300);
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
