import { expect, test, type Api } from "../support/fixtures.ts";

// Accounting Sweden (wave 2, finance): a VAT report shows the return it prepares. A draft is
// computed from the posted ledger by the VAT codes and boxes the user set up
// (vat_computation.compute), and the page shows box 49 first, the boxes in the sections of the
// form with their amounts in the company's currency, and that nothing is verified against SKV 4700.

async function withPackages(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  const installed = (await api.run("root", "module.list")).map((m: any) => m.id);
  const added: string[] = [];
  for (const id of ["accounting", "accounting_se"])
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

/** The box with this number, made when the instance has none (box numbers are unique). */
async function box(api: Api, n: string, label: string, kind: string) {
  const had = (
    await api.run("root", "accounting_se_vat_box.search", {
      filter: [{ field: "box", op: "eq", value: n }],
    })
  ).rows[0];
  return had ?? api.create("root", "accounting_se_vat_box", { box: n, label, kind });
}

test.describe("Accounting Sweden: the VAT return", () => {
  test("a draft report is computed from the ledger and shows box 49 and the boxes of the form", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, async () => {
      const tag = uid("S").replace(/\s+/g, "");
      const company = await api.create("root", "accounting_se_company", {
        name: uid("Verktyg AB"),
        currency: "SEK",
        fiscal_year_start_month: 1,
        vat_period: "quarterly",
      });
      const account = (code: string, name: string, account_type: string) =>
        api.create("root", "accounting_account", {
          code: `${tag}-${code}`,
          name: `${name} ${tag}`,
          account_type,
        });
      const bank = await account("1930", "Bank", "asset_cash");
      const sales = await account("3001", "Sales 25 %", "income");
      const output = await account("2611", "Output VAT 25 %", "liability_current");
      const tax = await api.create("root", "accounting_tax", {
        name: `Moms 25 % ${tag}`,
        rate_bp: 2500,
        scope: "sales",
        tax_account_id: output.id,
      });
      const b05 = await box(api, "05", "Momspliktig forsaljning", "base");
      const b10 = await box(api, "10", "Utgaende moms 25 %", "output");
      await box(api, "49", "Moms att betala eller fa tillbaka", "result");
      await api.create("root", "accounting_se_vat_code", {
        code: `S${tag.slice(-8)}`,
        label: "Sale in Sweden, 25 % VAT",
        treatment: "sale_domestic",
        rate_bp: 2500,
        tax_id: tax.id,
        base_box_id: b05.id,
        base_side: "credit",
        vat_box_id: b10.id,
        vat_account_id: output.id,
        vat_side: "credit",
      });
      const journal = await api.create("root", "accounting_journal", {
        name: uid("Kundfakturor"),
        code: tag,
        kind: "sale",
        currency: "SEK",
      });
      const move = await api.create("root", "accounting_move", {
        journal_id: journal.id,
        date: "2031-02-10",
        reference: uid("Faktura"),
      });
      const line = (account_id: string, debit: number, credit: number, tax_id?: string) =>
        api.create("root", "accounting_move_line", {
          move_id: move.id,
          account_id,
          debit,
          credit,
          ...(tax_id ? { tax_id } : {}),
        });
      await line(bank.id, 1250000, 0);
      await line(sales.id, 0, 1000000, tax.id);
      await line(output.id, 0, 250000, tax.id);
      await api.run("root", "accounting_move.post", { id: move.id });
      const name = uid("2031-Q1");
      await api.create("root", "accounting_se_vat_report", {
        name,
        period_kind: "quarter",
        start_on: "2031-01-01",
        end_on: "2031-03-31",
        company_id: company.id,
      });

      await signIn("root", "/accounting_se/accounting_se_vat_report?view=list");
      await page.getByText(name, { exact: true }).first().click();
      const ret = page.locator("section.vat-return");
      await expect(ret).toContainText("Experimental: not verified against SKV 4700");
      await expect(ret.locator(".vat-49")).toContainText("No VAT to pay");

      await ret.getByRole("button", { name: "Compute from ledger" }).click();
      await expect(page.locator(".toast").filter({ hasText: "Computed from" })).toBeVisible();
      await expect(ret.locator(".vat-49")).toContainText("VAT to pay");
      await expect(ret.locator(".vat-49-amount")).toContainText("2,500.00");
      const row05 = ret
        .locator(".vat-row")
        .filter({ has: page.locator(".vat-box-no", { hasText: /^05$/ }) });
      await expect(row05).toContainText("10,000.00");
      await expect(row05).toContainText("SEK");
      const row10 = ret
        .locator(".vat-row")
        .filter({ has: page.locator(".vat-box-no", { hasText: /^10$/ }) });
      await expect(row10).toContainText("2,500.00");
      await expect(row10).toContainText("Unverified");

      // What the page shows is what the server holds.
      const report = (
        await api.run("root", "accounting_se_vat_report.search", {
          filter: [{ field: "name", op: "eq", value: name }],
        })
      ).rows[0];
      expect(report.vat_to_pay).toBe(250000);
      expect(report.state).toBe("draft");
    });
  });

  test("a VAT code that feeds a box without a tax is named, with the way to fix it, before computing", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withPackages(api, async () => {
      const tag = uid("U").replace(/\s+/g, "");
      const b05 = await box(api, "05", "Momspliktig forsaljning", "base");
      const code = `U${tag.slice(-8)}`;
      const untaxed = await api.create("root", "accounting_se_vat_code", {
        code,
        label: "Sale without a tax yet",
        treatment: "sale_domestic",
        rate_bp: 2500,
        base_box_id: b05.id,
        base_side: "credit",
      });
      try {
        const name = uid("2032-Q1");
        await api.create("root", "accounting_se_vat_report", {
          name,
          period_kind: "quarter",
          start_on: "2032-01-01",
          end_on: "2032-03-31",
        });
        await signIn("root", "/accounting_se/accounting_se_vat_report?view=list");
        await page.getByText(name, { exact: true }).first().click();
        const ret = page.locator("section.vat-return");
        const notice = ret.locator(".notice, [role=status], [role=alert]").filter({
          hasText: "Some VAT codes cannot be computed yet",
        });
        await expect(notice).toContainText(code);
        await expect(notice.getByRole("link", { name: "Open VAT codes" })).toBeVisible();
        await expect(ret.getByRole("button", { name: "Compute from ledger" })).toBeDisabled();
        // The server says the same in its own words.
        const refused = await api.post(
          "/api/vat_computation/compute",
          {
            report: (
              await api.run("root", "accounting_se_vat_report.search", {
                filter: [{ field: "name", op: "eq", value: name }],
              })
            ).rows[0].id,
          },
          await api.token("root"),
        );
        expect(refused.status).toBe(400);
        expect(JSON.stringify(refused.json)).toContain(
          `code ${code}: maps to a box but has no tax_id`,
        );
      } finally {
        await api.run("root", "accounting_se_vat_code.delete", { id: untaxed.id });
      }
    });
  });
});
