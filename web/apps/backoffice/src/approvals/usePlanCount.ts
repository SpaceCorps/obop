// The count of plans waiting for the person (`plan.count`), for the ways into the Approvals page
// (the top-bar button and the account-menu item). Read on mount, when the window gets focus, when
// the tab becomes visible, after the page decided a plan (`plansChanged`), and every 60 seconds
// while the tab is visible. Nothing is read while the tab is hidden.
import { useCallback, useEffect, useState } from "react";
import { planCount, type PlanCount } from "./planApi.ts";

/** How often the count is read again while the tab is visible. */
export const COUNT_EVERY_MS = 60_000;

const bus = typeof EventTarget === "function" ? new EventTarget() : null;
const CHANGED = "obop-plans-changed";

/** Say that plans changed (one was decided here): every count reads again. */
export function plansChanged(): void {
  bus?.dispatchEvent(new Event(CHANGED));
}

/** Whether the entry to the page is shown: the person can decide, or something is waiting. */
export const showsApprovals = (c: PlanCount | null): boolean =>
  !!c && (c.can_decide || c.waiting > 0);

/** The badge's text: the number, or "9+" above nine. */
export const badgeText = (n: number): string => (n > 9 ? "9+" : String(n));

/**
 * The person's count, or null until the first answer (and after a refusal: an agent or an older
 * server has no `plan.count`, and then no entry is shown).
 */
export function usePlanCount(token: string): PlanCount | null {
  const [count, setCount] = useState<PlanCount | null>(null);
  const read = useCallback(async () => {
    if (typeof document !== "undefined" && document.visibilityState === "hidden") return;
    const r = await planCount(token);
    if (r.ok) setCount(r.value);
    else if (r.status !== 0) setCount(null);
  }, [token]);
  useEffect(() => {
    let live = true;
    const on = () => {
      if (live) void read();
    };
    on();
    const timer = setInterval(on, COUNT_EVERY_MS);
    addEventListener("focus", on);
    document.addEventListener("visibilitychange", on);
    bus?.addEventListener(CHANGED, on);
    return () => {
      live = false;
      clearInterval(timer);
      removeEventListener("focus", on);
      document.removeEventListener("visibilitychange", on);
      bus?.removeEventListener(CHANGED, on);
    };
  }, [read]);
  return count;
}
