// journals.ts: the figures of the Accounting overview's journal cards.
import { describe, expect, it } from "vite-plus/test";
import type { AggGroup, Row } from "../../api.ts";
import { journalCards, ledgerBalances } from "./journals.ts";

const g = (key: Record<string, unknown>, count: number, measures: Record<string, number>) =>
  ({ key, count, measures }) as AggGroup;

const journals: Row[] = [
  {
    id: "inv",
    name: "Customer invoices",
    code: "INV",
    kind: "sale",
    currency: "EUR",
    active: true,
  },
  { id: "bnk", name: "Bank", code: "BNK", kind: "bank", default_account_id: "1200", active: true },
  { id: "old", name: "Closed bank", code: "BNK0", kind: "bank", active: false },
  { id: "se", name: "Bank SE", code: "SBNK", kind: "bank", currency: "SEK", active: true },
];

describe("journalCards", () => {
  it("puts bank journals first, leaves inactive ones out and sums per state", () => {
    const cards = journalCards(
      journals,
      [
        g({ journal_id: "inv", state: "draft" }, 2, { "sum:total_debit": 300 }),
        g({ journal_id: "inv", state: "posted" }, 5, { "sum:total_debit": 1000 }),
        g({ journal_id: "bnk", state: "draft" }, 1, { "sum:total_debit": 50 }),
      ],
      [
        g({ journal_id: "bnk", state: "unmatched" }, 3, { "sum:amount": -120 }),
        g({ journal_id: "bnk", state: "reconciled" }, 7, { "sum:amount": 900 }),
      ],
      "EUR",
      new Map([["1200", 4200]]),
    );
    expect(cards.map((c) => c.code)).toEqual(["BNK", "SBNK", "INV"]);
    const bank = cards[0]!;
    expect(bank.currency).toBe("EUR");
    expect(bank.toMatch).toEqual({ count: 3, sum: -120 });
    expect(bank.drafts).toEqual({ count: 1, sum: 50 });
    expect(bank.ledgerBalance).toBe(4200);
    expect(bank.statementLines).toBe(10);
    expect(cards[1]!.currency).toBe("SEK");
    expect(cards[1]!.toMatch.count).toBe(0);
    const sales = cards[2]!;
    expect(sales.drafts).toEqual({ count: 2, sum: 300 });
    expect(sales.posted).toEqual({ count: 5, sum: 1000 });
  });
});

describe("ledgerBalances", () => {
  it("counts only the entries it is given (posted and reversed), debit minus credit", () => {
    const out = ledgerBalances(
      [
        g({ account_id: "1200", move_id: "a" }, 1, { "sum:debit": 500, "sum:credit": 0 }),
        g({ account_id: "1200", move_id: "b" }, 1, { "sum:debit": 0, "sum:credit": 200 }),
        g({ account_id: "1200", move_id: "draft" }, 1, { "sum:debit": 9999, "sum:credit": 0 }),
      ],
      new Set(["a", "b"]),
    );
    expect(out.get("1200")).toBe(300);
  });
});
