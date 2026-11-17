import { expect, test, type Api } from "../support/fixtures.ts";

// Stream k2 (owner rulings of 2026-11-17): a posted invoice books its draft journal entry from the
// company's booking settings (Accounting > Invoice booking), a booked bank payment can be taken back
// in one step ("Undo booking", bank_matching.unbook), and the person who made an expense claim may
// approve it only when the company setting allows it (Expenses > Settings).

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const installed = (await api.run("root", "module.list")).map((m: any) => m.id);
  const added: string[] = [];
  for (const id of ids)
    if (!installed.includes(id)) {
      await api.run("root", "marketplace_listing.install", { id });
      added.push(id);
    }
  try {
    await run();
  } finally {
    for (const id of added.reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

/** The booking settings are company-wide: leave them as the rest of the suite expects (unset). */
async function clearBooking(api: Api) {
  await api.run("root", "invoice_posting_setting.update", {
    book_on_post: true,
    journal: "",
    receivable_account: "",
    revenue_account: "",
    tax: "",
    vat_account: "",
  });
}

test.describe("finance flows of stream k2", () => {
  test("posting an invoice books its draft entry with the settings chosen on the booking page", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, ["accounting"], async () => {
      const tag = uid("K").replace(/\s+/g, "");
      const account = (code: string, name: string, account_type: string) =>
        api.create("root", "accounting_account", { code, name, account_type });
      const rec = await account(`R${tag}`, uid("Receivables"), "asset_receivable");
      const rev = await account(`S${tag}`, uid("Sales"), "income");
      const vat = await account(`V${tag}`, uid("Output VAT"), "liability_current");
      const journal = await api.create("root", "accounting_journal", {
        name: uid("Sales journal"),
        code: `J${tag}`,
        kind: "sale",
      });
      const tax = await api.create("root", "accounting_tax", {
        name: uid("VAT 25"),
        rate_bp: 2500,
        scope: "sales",
        tax_account_id: vat.id,
      });
      try {
        await signIn("root", "/accounting/invoice-booking");
        await expect(page.getByRole("heading", { name: "Invoice booking" })).toBeVisible();
        await expect(
          page.getByRole("switch", { name: "Book invoices when they are posted" }),
        ).toBeChecked();
        await page.getByLabel("Journal").selectOption(journal.id);
        await page.getByLabel("Receivable account").selectOption(rec.id);
        await page.getByLabel("Revenue account").selectOption(rev.id);
        await page.getByLabel("Tax", { exact: true }).selectOption(tax.id);
        await page.getByRole("button", { name: "Save" }).click();
        await expect(page.getByText("Booking settings saved")).toBeVisible();
        const s = await api.run("root", "invoice_posting_setting.get", {});
        expect(s.ready).toBe(true);

        const party = await api.create("root", "party", { name: uid("Kunde") });
        const inv = await api.create("root", "invoice", {
          party_id: party.id,
          issue_date: "2026-11-02",
        });
        await api.create("root", "invoice_line", {
          invoice_id: inv.id,
          description: "Consulting",
          quantity: 2,
          unit_price_minor: 10000,
        });
        await api.run("root", "invoice.post", { id: inv.id });
        const link = await api.run("root", "invoice_posting.get", { invoice: inv.id });
        expect(link.booked).toBe(true);
        expect(link.move_state).toBe("draft");
        const move = await api.run("root", "accounting_move.get", { id: link.move_id });
        expect(move.total_debit).toBe(25000);
        expect(move.date).toBe("2026-11-02");

        await page.goto(`/#/invoicing/invoice?id=${inv.id}`);
        await expect(
          page.getByText("Booked to the ledger as a draft entry. An approver posts it."),
        ).toBeVisible();
        await expect(page.getByRole("link", { name: "Open entry" })).toBeVisible();
      } finally {
        await clearBooking(api);
      }
    });
  });

  test("a booked bank payment is taken back with Undo booking", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, ["accounting"], async () => {
      const tag = uid("B").replace(/\s+/g, "");
      const bank = await api.create("root", "accounting_account", {
        code: `B${tag}`,
        name: uid("Bank"),
        account_type: "asset_cash",
      });
      const income = await api.create("root", "accounting_account", {
        code: `I${tag}`,
        name: uid("Other income"),
        account_type: "income_other",
      });
      const journal = await api.create("root", "accounting_journal", {
        name: uid("Bank journal"),
        code: `BJ${tag}`,
        kind: "bank",
        default_account_id: bank.id,
      });
      const line = await api.create("root", "accounting_bank_statement_line", {
        journal_id: journal.id,
        date: "2026-11-03",
        label: uid("Refund from supplier"),
        amount: 4200,
        state: "unmatched",
      });
      const booked = await api.run("root", "bank_matching.book", {
        bank_line: line.id,
        counter_account: income.id,
      });
      await signIn("root", `/accounting/accounting_bank_statement_line?id=${line.id}`);
      await page.getByRole("button", { name: "Undo booking" }).click();
      await page.getByRole("alertdialog").getByRole("button", { name: "Undo booking" }).click();
      await expect(page.getByText("Booking undone")).toBeVisible();
      await expect(page.getByRole("button", { name: "Undo booking" })).toHaveCount(0);
      const move = await api.run("root", "accounting_move.get", { id: booked.move_id });
      expect(move.state).toBe("cancelled");
      const link = await api.run("root", "bank_matching.get", { bank_line: line.id });
      expect(link.booked).toBe(false);
    });
  });

  test("the maker of a claim approves it only when the company allows it", async ({
    page,
    signIn,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await withPackages(api, ["expenses"], async () => {
      const claim = await api.create("root", "expenses_expense", {
        description: "Parking at the fair",
        amount: 1800,
        category: "travel",
      });
      await api.run("root", "expenses_expense.submit", { id: claim.id });
      try {
        await signIn("root", `/expenses/expenses_expense?id=${claim.id}`);
        // The record's own action button (the kanban card behind it has one too).
        await page.locator('button.btn.primary[title^="Approve a submitted claim"]').click();
        const ask = page.getByRole("alertdialog");
        if (await ask.isVisible().catch(() => false))
          await ask.getByRole("button", { name: "Approve" }).click();
        await expect(page.getByText("someone else has to approve it").first()).toBeVisible();
        expect((await api.run("root", "expenses_expense.get", { id: claim.id })).state).toBe(
          "submitted",
        );

        await page.goto("/#/expenses/settings");
        const allow = page.getByRole("switch", { name: "People may approve their own claims" });
        await expect(allow).not.toBeChecked();
        await allow.check();
        await expect(page.getByText("Expense settings saved")).toBeVisible();
        expect((await api.run("root", "expenses_setting.get", {})).allow_self_approval).toBe(true);
        await api.run("root", "expenses_expense.approve", { id: claim.id });
        expect((await api.run("root", "expenses_expense.get", { id: claim.id })).state).toBe(
          "approved",
        );
      } finally {
        await api.run("root", "expenses_setting.update", { allow_self_approval: false });
      }
    });
  });
});
