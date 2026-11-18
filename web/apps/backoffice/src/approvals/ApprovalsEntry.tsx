// The ways into the Approvals page from the shell: the top-bar button with the count of what waits
// for the person, and the account menu's item (the way in on a phone whose bar is full). Both only
// open the page: nothing is decided from the bar (rule 10).
import { useEffect, useRef, useState } from "react";
import { Icon, t } from "@obop/ui";
import type { PlanCount } from "./planApi.ts";
import { badgeText } from "./usePlanCount.ts";
import { approvalsHref } from "./route.ts";
import "./entry.css";

/** The name of the entry, for a screen reader and the tooltip: the count in words. */
export function entryName(count: PlanCount): string {
  return count.waiting > 0
    ? t("Approvals, {n} waiting for you", { n: count.waiting })
    : t("Approvals, nothing waiting");
}

/**
 * The button: a check-circle with the count in a badge ("9+" above nine). The change of the count
 * is announced politely, once per change, never when the first count arrives.
 */
export function ApprovalsButton({ count }: { count: PlanCount }) {
  const name = entryName(count);
  const previous = useRef<number | null>(null);
  const [said, setSaid] = useState("");
  useEffect(() => {
    if (previous.current !== null && previous.current !== count.waiting) setSaid(name);
    previous.current = count.waiting;
  }, [count.waiting, name]);
  return (
    <>
      <a className="icon-btn appr-entry" href={approvalsHref()} aria-label={name} title={name}>
        <Icon name="check-circle" size={20} />
        {count.waiting > 0 && (
          <span className="appr-badge" aria-hidden="true">
            {badgeText(count.waiting)}
          </span>
        )}
      </a>
      <span className="sr-only" role="status" aria-live="polite">
        {said}
      </span>
    </>
  );
}
