/**
 * A floating list or calendar tied to an input. It is drawn in the browser's top layer (the
 * Popover API, `popover="manual"`), so no panel footer, dialog or `overflow: clip` cuts it off,
 * while it stays a child of the input in the DOM: focus traps, clicks and screen readers treat it
 * as part of the field. It sits under the input and flips above when there is not room below.
 */
import { useLayoutEffect, type RefObject } from "react";

type Options = {
  /** The tallest it may grow, in CSS pixels. */
  maxHeight?: number;
  /** The narrowest it may be; it is at least as wide as the input. */
  minWidth?: number;
  /** Keep the width of the input exactly (lists) rather than its own (calendars). */
  matchWidth?: boolean;
};

const canPop = (
  el: HTMLElement,
): el is HTMLElement & { showPopover(): void; hidePopover(): void } =>
  typeof (el as { showPopover?: unknown }).showPopover === "function";

/** Place `pop` against `anchor` while `open`, and keep it there on scroll and resize. */
export function useAnchored(
  open: boolean,
  anchor: RefObject<HTMLElement | null>,
  pop: RefObject<HTMLElement | null>,
  { maxHeight = 320, minWidth = 0, matchWidth = true }: Options = {},
): void {
  useLayoutEffect(() => {
    const el = pop.current;
    const at = anchor.current;
    if (!open || !el || !at) return;
    const top = canPop(el);
    if (top) {
      try {
        el.showPopover();
      } catch {
        /* already shown, or not connected yet */
      }
    }
    const place = () => {
      const r = at.getBoundingClientRect();
      const vw = document.documentElement.clientWidth || innerWidth;
      const vh = innerHeight;
      const gap = 4;
      const below = vh - r.bottom - gap - 8;
      const above = r.top - gap - 8;
      const natural = Math.min(el.scrollHeight || maxHeight, maxHeight);
      const flip = below < Math.min(natural, 180) && above > below;
      const room = Math.max(96, Math.floor(flip ? above : below));
      el.style.maxBlockSize = `${Math.min(maxHeight, room)}px`;
      const width = Math.min(Math.max(r.width, minWidth), vw - 16);
      if (matchWidth) el.style.inlineSize = `${width}px`;
      else el.style.minInlineSize = `${Math.min(Math.max(r.width, minWidth), vw - 16)}px`;
      const w = matchWidth ? width : el.offsetWidth || width;
      const left = Math.max(8, Math.min(r.left, vw - w - 8));
      el.style.left = `${left}px`;
      el.style.right = "auto";
      if (flip) {
        el.style.top = "auto";
        el.style.bottom = `${vh - r.top + gap}px`;
      } else {
        el.style.bottom = "auto";
        el.style.top = `${r.bottom + gap}px`;
      }
      el.dataset.side = flip ? "top" : "bottom";
    };
    place();
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    ro?.observe(el);
    ro?.observe(at);
    addEventListener("scroll", place, true);
    addEventListener("resize", place);
    return () => {
      ro?.disconnect();
      removeEventListener("scroll", place, true);
      removeEventListener("resize", place);
      if (top) {
        try {
          el.hidePopover();
        } catch {
          /* already hidden */
        }
      }
    };
  }, [open, anchor, pop, maxHeight, minWidth, matchWidth]);
}
