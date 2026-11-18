// What a person is told when an agent's or their own change was sent for approval: one sentence and,
// where there is a toast, a link to that plan. Never the plan's id as text (nobody can use eight
// characters), and never a control that decides: the link only opens the page (rule 10).
import { t, type ToastAction } from "@obop/ui";
import { approvalsHref } from "./route.ts";
import { plansChanged } from "./usePlanCount.ts";

type Notify = (text: string, tone?: "info", opts?: { action?: ToastAction }) => void;

/** The sentence, for a place that has no toast (an inline message). */
export const sentText = (): string => t("Sent for approval.");

/**
 * Several plans from one act (five deletes at once): one toast that says how many and links to the
 * list, not to the first of them. A toast only links; it never decides (rule 10).
 */
export function notifySentMany(notify: Notify, count: number): void {
  notify(
    t("{count, plural, one {# request sent for approval.} other {# requests sent for approval.}}", {
      count,
    }),
    "info",
    { action: { label: t("Open Approvals"), href: approvalsHref() } },
  );
  plansChanged();
}

/** The toast: the sentence and "Open Approvals", a link to the plan. */
export function notifySent(notify: Notify, planId: string): void {
  notify(sentText(), "info", {
    action: { label: t("Open Approvals"), href: approvalsHref({ id: planId }) },
  });
  // The bar's count reads again: a plan has just been proposed.
  plansChanged();
}
