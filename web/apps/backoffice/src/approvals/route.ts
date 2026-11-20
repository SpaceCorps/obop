// Where the Approvals page lives: `#/approvals`, `#/approvals?view=decided` and
// `#/approvals?id=<plan>` (a deep link that opens one plan). A page of the shell like Customize,
// not an app of a module: the `app` part of the address is the page's name and `model` is empty.
import { href, type Route } from "../route.ts";

/** The `app` part of the page's address. */
export const APPROVALS = "approvals";

/** Whether the address is the Approvals page. */
export const isApprovals = (r: Pick<Route, "app">): boolean => r.app === APPROVALS;

/** Which list the page shows: the plans waiting for the person, or the decided ones. */
export type ApprovalsTab = "waiting" | "decided";
export const tabOf = (r: Pick<Route, "view">): ApprovalsTab =>
  r.view === "decided" ? "decided" : "waiting";

/** The address of the page, of its Decided list, or of one plan (a link from a toast or elsewhere). */
export function approvalsHref(opts: { id?: string; tab?: ApprovalsTab } = {}): string {
  return href({
    app: APPROVALS,
    ...(opts.tab === "decided" ? { view: "decided" } : {}),
    ...(opts.id ? { id: opts.id } : {}),
  });
}
