import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Icon, rovingKeys, type MenuItem } from "@obop/ui";

/** Open a card's menu from outside its button: a long press, or the context menu key. */
export type CardMenuHandle = { open: () => void };

/**
 * The ⋯ menu of a card. A board column scrolls, and a menu drawn inside it would be cut off at the
 * column's edge, so the list is drawn over the page (a portal) at the button's place: under it, or
 * above it when there is no room below, and inside the screen sideways. Arrow keys move, Enter
 * picks, Escape closes and returns focus to the button; a click outside, a scroll or a resize
 * closes it. The items are the shared menu items of @obop/ui (a disabled item says why).
 */
export function CardMenu({
  label,
  items,
  handle,
  className = "kmore-btn",
  trigger,
}: {
  /** The button's accessible name and the menu's ("Move SO-00028 to"). */
  label: string;
  items: MenuItem[];
  handle?: { current: CardMenuHandle | null };
  className?: string;
  /** What the button shows; the ⋯ icon by default. */
  trigger?: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState<{ top: number; left: number } | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const id = useId();
  const close = useCallback((refocus = true) => {
    setOpen(false);
    setPlace(null);
    if (refocus) btn.current?.focus();
  }, []);
  if (handle) handle.current = { open: () => setOpen(true) };

  // Placed once the list is drawn and its size is known.
  useLayoutEffect(() => {
    if (!open) return;
    const b = btn.current?.getBoundingClientRect();
    const l = list.current;
    if (!b || !l) return;
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    const w = l.offsetWidth;
    const h = l.offsetHeight;
    const rtl = getComputedStyle(btn.current!).direction === "rtl";
    let left = rtl ? b.left : b.right - w;
    left = Math.max(8, Math.min(left, vw - w - 8));
    const below = b.bottom + 6;
    const top = below + h <= vh - 8 || b.top - h - 6 < 8 ? below : b.top - h - 6;
    setPlace({ top: Math.max(8, top), left });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    list.current?.querySelector<HTMLElement>('[role="menuitem"]:not([disabled])')?.focus();
    const outside = (e: PointerEvent) => {
      const path = e.composedPath();
      if (list.current && !path.includes(list.current) && !path.includes(btn.current!))
        close(false);
    };
    // A scroll inside the list itself (a long menu) keeps it open.
    const scrolled = (e: Event) => {
      if (list.current && e.target instanceof Node && list.current.contains(e.target)) return;
      close(false);
    };
    const resized = () => close(false);
    addEventListener("pointerdown", outside, true);
    addEventListener("scroll", scrolled, true);
    addEventListener("resize", resized);
    return () => {
      removeEventListener("pointerdown", outside, true);
      removeEventListener("scroll", scrolled, true);
      removeEventListener("resize", resized);
    };
  }, [open, close]);

  return (
    <>
      <button
        ref={btn}
        type="button"
        className={className}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={label}
        title={label}
        draggable={false}
        onClick={(e) => {
          e.stopPropagation();
          if (open) close();
          else setOpen(true);
        }}
        onKeyDown={(e) => {
          // The card's own keys (Enter opens, arrows move) stop at its button.
          e.stopPropagation();
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            setOpen(true);
          }
        }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        {trigger ?? <Icon name="more" size={16} />}
      </button>
      {open &&
        createPortal(
          <div
            id={id}
            ref={list}
            role="menu"
            aria-label={label}
            className="panel menu-list kmenu"
            style={
              place
                ? { top: place.top, left: place.left }
                : { top: 0, left: 0, visibility: "hidden" }
            }
            // A portal still bubbles through the React tree: the card must not see these.
            onClick={(e) => e.stopPropagation()}
            onPointerDown={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              e.stopPropagation();
              rovingKeys(e, '[role="menuitem"]', "vertical", false);
              if (e.key === "Escape") {
                e.preventDefault();
                close();
              }
              if (e.key === "Tab") close(false);
            }}
          >
            {items.map((it, i) =>
              it === "separator" ? (
                <hr key={`s${i}`} className="menu-sep" />
              ) : "heading" in it ? (
                <div key={`h${i}`} className="menu-head" role="presentation">
                  {it.heading}
                </div>
              ) : "items" in it ? null /* a card's menu has no submenus */ : (
                <button
                  key={`${i}:${it.label}`}
                  type="button"
                  role="menuitem"
                  disabled={it.disabled}
                  title={it.description}
                  className={`menu-item${it.danger ? " danger" : ""}${it.description ? " described" : ""}`}
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
                </button>
              ),
            )}
          </div>,
          document.body,
        )}
    </>
  );
}
