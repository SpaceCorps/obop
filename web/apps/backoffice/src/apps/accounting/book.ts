// The accounts the Match transactions screen offers to book a bank line to. Pure, so the rule is
// tested without a server.
import type { Row } from "../../api.ts";

/**
 * Every account but the journal's own (the bank side of the entry). While an invoice that is not
 * in the ledger is being paid, receivable accounts are left out too: no receivable was ever
 * debited for that invoice, and the server refuses to credit one with its payment.
 */
export function bookAccounts(
  accounts: readonly Row[],
  journalAccountId: unknown,
  payingUnbookedInvoice: boolean,
): Row[] {
  return accounts.filter(
    (a) =>
      a.id !== journalAccountId &&
      !(payingUnbookedInvoice && a.account_type === "asset_receivable"),
  );
}
