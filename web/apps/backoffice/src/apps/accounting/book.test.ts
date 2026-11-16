import { describe, expect, it } from "vite-plus/test";
import { bookAccounts } from "./book.ts";

const accounts = [
  { id: "bank", code: "1200", account_type: "asset_cash" },
  { id: "ar", code: "1400", account_type: "asset_receivable" },
  { id: "ar-se", code: "1510", account_type: "asset_receivable" },
  { id: "rev", code: "8400", account_type: "income" },
];

describe("the accounts a bank line can be booked to", () => {
  it("leave out the journal's own account", () => {
    expect(bookAccounts(accounts, "bank", false).map((a) => a.id)).toEqual(["ar", "ar-se", "rev"]);
  });

  it("leave out receivables while an invoice that is not in the ledger is paid", () => {
    expect(bookAccounts(accounts, "bank", true).map((a) => a.id)).toEqual(["rev"]);
  });
});
