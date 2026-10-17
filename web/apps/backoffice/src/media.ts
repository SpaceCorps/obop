import { useEffect, useState } from "react";

/** Whether a media query matches now, and keeps matching as the window changes. */
export function useMedia(query: string): boolean {
  const [on, setOn] = useState(() => matchMedia(query).matches);
  useEffect(() => {
    const m = matchMedia(query);
    const change = () => setOn(m.matches);
    change();
    m.addEventListener("change", change);
    return () => m.removeEventListener("change", change);
  }, [query]);
  return on;
}

/** The record opens as a sheet over the list up to this width (D2): tablets and phones. */
export const SHEET_QUERY = "(max-width: 64rem)";
/** Full-screen up to this width. */
export const PHONE_QUERY = "(max-width: 40rem)";

/**
 * Stops the page behind a sheet from scrolling, and puts the scroll position back when it closes.
 * The list underneath stays mounted, so its selection and scroll are kept as well.
 */
export function useScrollLock(active: boolean) {
  useEffect(() => {
    if (!active) return;
    const y = window.scrollY;
    const root = document.documentElement;
    root.classList.add("sheet-open");
    return () => {
      root.classList.remove("sheet-open");
      window.scrollTo({ top: y });
    };
  }, [active]);
}
