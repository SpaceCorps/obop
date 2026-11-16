import { expect, test } from "../support/fixtures.ts";

// Invoicing (wave 2, finance): a Print button on an invoice opens it as a sheet of paper with the
// customer, the lines and the total, and "Print or save as PDF" opens the browser's print dialog.
// In print only the sheet goes to paper. It reads through the search commands only.

test.describe("Invoicing: the printable invoice", () => {
  test("an invoice prints as a sheet with its customer, lines and total", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const customer = await api.create("root", "party", {
      name: uid("Print customer"),
      email: "billing@print-customer.example",
      tax_id: "DE123456789",
    });
    const invoice = await api.create("root", "invoice", { party_id: customer.id, currency: "EUR" });
    const bench = uid("Workbench with vice");
    await api.create("root", "invoice_line", {
      invoice_id: invoice.id,
      description: bench,
      quantity: 2,
      unit_price_minor: 34900,
    });
    await api.create("root", "invoice_line", {
      invoice_id: invoice.id,
      description: uid("Pull saw"),
      quantity: 4,
      unit_price_minor: 3990,
    });
    // Count the calls of the print dialog instead of opening it.
    await page.addInitScript(() => {
      (window as unknown as { printed: number }).printed = 0;
      window.print = () => void (window as unknown as { printed: number }).printed++;
    });

    await signIn("root", `/invoicing/invoice?id=${invoice.id}&full=1`);
    await page.getByRole("button", { name: "Print", exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`#/invoicing/print\\?id=${invoice.id}`));

    const sheet = page.locator("article.inv-sheet");
    await expect(sheet).toContainText(customer.name);
    await expect(sheet).toContainText("billing@print-customer.example");
    await expect(sheet).toContainText("VAT ID DE123456789");
    await expect(sheet).toContainText(bench);
    await expect(sheet.locator(".inv-lines tbody tr")).toHaveCount(2);
    await expect(sheet.locator(".inv-total")).toContainText("€857.60");
    // Not posted yet: the sheet says it is not a valid invoice.
    await expect(sheet.locator(".inv-stamp")).toContainText("Draft: not a valid invoice");

    await page.getByRole("button", { name: "Print or save as PDF" }).click();
    expect(await page.evaluate(() => (window as unknown as { printed: number }).printed)).toBe(1);

    // On paper: the sheet and nothing of the app around it.
    await page.emulateMedia({ media: "print" });
    await expect(sheet).toBeVisible();
    await expect(page.locator(".topbar")).toBeHidden();
    await expect(page.locator(".inv-toolbar")).toBeHidden();
    await page.emulateMedia({ media: "screen" });

    await page.getByRole("button", { name: "Back to the invoice" }).click();
    await expect(page).toHaveURL(new RegExp(`#/invoicing/invoice\\?id=${invoice.id}`));
  });

  test("on a phone the sheet fits the screen", async ({ page, signIn, api, uid }) => {
    await page.setViewportSize({ width: 320, height: 720 });
    const customer = await api.create("root", "party", { name: uid("Phone print customer") });
    const invoice = await api.create("root", "invoice", { party_id: customer.id });
    await api.create("root", "invoice_line", {
      invoice_id: invoice.id,
      description: uid("A rather long description of a service that wraps on a phone"),
      quantity: 1,
      unit_price_minor: 125000,
    });
    await signIn("root", `/invoicing/print?id=${invoice.id}`);
    await expect(page.locator("article.inv-sheet")).toContainText(customer.name);
    const wide = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
    );
    expect(wide, "no sideways scroll at 320 px").toBe(false);
  });
});
