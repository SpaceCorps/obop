import { href } from "../../route.ts";

export const BANK_LINE = "accounting_bank_statement_line";
/** The `view` of the bank transactions page that shows the reconcile screen. */
export const RECONCILE = "reconcile";

/** The reconcile screen, for one bank journal when `journal` is given (kept in `section`). */
export const reconcileHref = (app: string, journal = ""): string =>
  href({ app, model: BANK_LINE, view: RECONCILE, section: journal });
