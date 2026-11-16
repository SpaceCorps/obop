import { expect, test, type Api } from "../support/fixtures.ts";

// Accounting (wave 2, finance): the app opens on one card per journal, and a bank journal's card
// leads to the Match transactions screen, where a bank transaction is matched to a payment that is
// already booked, or booked to an account as a draft entry and matched in one click. Every step is
// a command: search with aggregate, bank_matching.book, and the update and match of the bank line.

/** The bundled accounting package, installed for this file and removed afterwards. */
async function withAccounting(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  const had = (await api.run("root", "module.list")).some((m: any) => m.id === "accounting");
  if (!had) await api.run("root", "marketplace_listing.install", { id: "accounting" });
  try {
    await run();
  } finally {
    if (!had) await api.post("/api/module/uninstall", { id: "accounting" }, root);
  }
}

/** A bank journal of its own, with its bank account and an expense account to book fees to. */
async function bankJournal(api: Api, uid: (p: string) => string) {
  const code = uid("B").replace(/\s+/g, "");
  const bank = await api.create("root", "accounting_account", {
    code: `${code}-1200`,
    name: uid("Bank account"),
    account_type: "asset_cash",
  });
  const fees = await api.create("root", "accounting_account", {
    code: `${code}-4970`,
    name: uid("Bank charges"),
    account_type: "expense",
  });
  const receivable = await api.create("root", "accounting_account", {
    code: `${code}-1400`,
    name: uid("Receivables"),
    account_type: "asset_receivable",
    reconcile: true,
  });
  const journal = await api.create("root", "accounting_journal", {
    name: uid("Bank journal"),
    code,
    kind: "bank",
    currency: "EUR",
    default_account_id: bank.id,
  });
  return { bank, fees, receivable, journal };
}

test.describe("Accounting: journal cards and matching bank transactions", () => {
  test("the overview card of a bank journal leads to its transactions, and one is booked and matched", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withAccounting(api, async () => {
      const { fees, journal } = await bankJournal(api, uid);
      const label = uid("Card terminal fees");
      const line = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-12",
        label,
        amount: -2380,
      });

      await signIn("root", "/accounting");
      const card = page.getByRole("article", { name: journal.name });
      await expect(card).toBeVisible();
      await expect(card).toContainText("To match");
      await expect(card).toContainText("−€23.80");
      await card.getByRole("link", { name: "Match transactions (1)" }).click();

      await expect(
        page.getByRole("heading", { level: 1 }).or(page.locator(".crumbs strong")),
      ).toContainText("Match transactions");
      const detail = page.locator(".acc-rec-detail");
      await expect(detail).toContainText(label);
      await expect(detail).toContainText("No open item has this amount");
      await detail.getByRole("combobox", { name: /^Account/ }).selectOption({ value: fees.id });
      await detail.getByRole("button", { name: "Book and match" }).last().click();
      await expect(
        page.locator(".toast").filter({ hasText: "Booked as a draft entry and matched" }),
      ).toBeVisible();
      await expect(page.getByText("All bank transactions are matched")).toBeVisible();

      // What the screen did, as the server holds it.
      const [after] = await api
        .run("root", "accounting_bank_statement_line.search", {
          filter: [{ field: "id", op: "eq", value: line.id }],
        })
        .then((v) => v.rows);
      expect(after.state).toBe("matched");
      const items = (
        await api.run("root", "accounting_move_line.search", {
          filter: [{ field: "id", op: "eq", value: after.move_line_id }],
        })
      ).rows;
      expect(items[0].credit).toBe(2380);
      const move = (
        await api.run("root", "accounting_move.search", {
          filter: [{ field: "id", op: "eq", value: items[0].move_id }],
        })
      ).rows[0];
      expect(move.state).toBe("draft");
      expect(move.journal_id).toBe(journal.id);
    });
  });

  test("a payment already booked on the bank account is the first proposal, and Enter matches it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withAccounting(api, async () => {
      const { bank, receivable, journal } = await bankJournal(api, uid);
      const reference = uid("Payment INV");
      const move = await api.create("root", "accounting_move", {
        journal_id: journal.id,
        date: "2026-11-10",
        reference,
      });
      const bankItem = await api.create("root", "accounting_move_line", {
        move_id: move.id,
        account_id: bank.id,
        label: reference,
        debit: 95200,
        credit: 0,
      });
      await api.create("root", "accounting_move_line", {
        move_id: move.id,
        account_id: receivable.id,
        label: reference,
        debit: 0,
        credit: 95200,
      });
      const label = uid("SEPA credit transfer");
      const line = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-10",
        label,
        amount: 95200,
      });

      await signIn(
        "root",
        `/accounting/accounting_bank_statement_line?view=reconcile&section=${journal.id}`,
      );
      const detail = page.locator(".acc-rec-detail");
      await expect(detail).toContainText(label);
      const first = detail.locator(".acc-prop").first();
      await expect(first).toContainText("Already booked on the bank account");
      await expect(first.getByRole("button", { name: "Match", exact: true })).toBeVisible();
      // Enter takes the first proposal (focus outside any field).
      await page.locator(".acc-rec-card").click();
      await page.keyboard.press("Enter");
      await expect(page.locator(".toast").filter({ hasText: "Matched" })).toBeVisible();
      await expect(page.getByText("All bank transactions are matched")).toBeVisible();

      const after = (
        await api.run("root", "accounting_bank_statement_line.search", {
          filter: [{ field: "id", op: "eq", value: line.id }],
        })
      ).rows[0];
      expect(after.state).toBe("matched");
      expect(after.move_line_id).toBe(bankItem.id);
    });
  });

  test("an open item that one payment settled is not offered to the next, and the server refuses a second settle", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withAccounting(api, async () => {
      const { fees, receivable, journal } = await bankJournal(api, uid);
      // An invoice item: the customer owes 952.00 on the receivables account.
      const reference = uid("INV");
      const move = await api.create("root", "accounting_move", {
        journal_id: journal.id,
        date: "2026-11-02",
        reference,
      });
      const item = await api.create("root", "accounting_move_line", {
        move_id: move.id,
        account_id: receivable.id,
        label: reference,
        debit: 95200,
        credit: 0,
      });
      await api.create("root", "accounting_move_line", {
        move_id: move.id,
        account_id: fees.id,
        label: reference,
        debit: 0,
        credit: 95200,
      });
      const first = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-10",
        label: uid("Payment one"),
        amount: 95200,
      });
      const second = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-11",
        label: uid("Payment two"),
        amount: 95200,
      });

      await signIn(
        "root",
        `/accounting/accounting_bank_statement_line?view=reconcile&section=${journal.id}`,
      );
      const detail = page.locator(".acc-rec-detail");
      await expect(detail).toContainText(first.label);
      const proposal = detail.locator(".acc-prop").first();
      await expect(proposal).toContainText("Open item: book the payment against it.");
      await expect(proposal).toContainText("€952.00");
      await page.locator(".acc-rec-card").click();
      await page.keyboard.press("Enter");
      await expect(
        page.locator(".toast").filter({ hasText: "Booked as a draft entry and matched" }),
      ).toBeVisible();

      // The next transaction of the same amount is not offered the paid item.
      await expect(detail).toContainText(second.label);
      await expect(detail).toContainText("No open item has this amount");

      const paid = (
        await api.run("root", "accounting_bank_statement_line.search", {
          filter: [{ field: "id", op: "eq", value: first.id }],
        })
      ).rows[0];
      expect(paid.state).toBe("matched");
      expect(paid.settles_line_id).toBe(item.id);
      // The rule is the server's: a second transaction cannot settle the same item.
      const refused = await api.post(
        "/api/accounting_bank_statement_line/update",
        { id: second.id, settles_line_id: item.id },
        await api.token("root"),
      );
      expect(refused.status).toBe(400);
      expect(JSON.stringify(refused.json)).toContain("unique");
    });
  });

  test("on a phone the transactions and the proposals are two screens", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await withAccounting(api, async () => {
      const { fees, journal } = await bankJournal(api, uid);
      const label = uid("Direct debit telephone");
      await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-13",
        label,
        amount: -8919,
      });
      await signIn(
        "root",
        `/accounting/accounting_bank_statement_line?view=reconcile&section=${journal.id}`,
      );
      await expect(page.locator(".acc-rec-detail")).toHaveCount(0);
      await page.getByRole("button", { name: new RegExp(label) }).click();
      const detail = page.locator(".acc-rec-detail");
      await expect(detail).toContainText(label);
      await detail.getByRole("combobox", { name: /^Account/ }).selectOption({ value: fees.id });
      await detail.getByRole("button", { name: "Book and match" }).last().click();
      await expect(page.getByText("All bank transactions are matched")).toBeVisible();
      const wide = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      );
      expect(wide, "no sideways scroll").toBe(false);
    });
  });
});
