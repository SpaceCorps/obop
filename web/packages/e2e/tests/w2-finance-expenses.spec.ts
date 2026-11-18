import { expect, test, type Api } from "../support/fixtures.ts";

// Expenses (wave 2, finance): an expense claim made on a phone from start to finish. New, what was
// spent, the amount and the category (the date and the person are filled in), Create, the photo of
// the receipt, then Submit. The form puts those fields first; everything is the generated create,
// the file upload and the submit action.

const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

async function withExpenses(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  const had = (await api.run("root", "module.list")).some((m: any) => m.id === "expenses");
  if (!had) await api.run("root", "marketplace_listing.install", { id: "expenses" });
  try {
    await run();
  } finally {
    if (!had) await api.post("/api/module/uninstall", { id: "expenses" }, root);
  }
}

test.describe("Expenses: a claim on a phone", () => {
  test("new claim, receipt photo and submit, with the main fields first", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await withExpenses(api, async () => {
      const what = uid("Taxi from the airport");
      await signIn("root", "/expenses");
      await page.locator(".cp-fab").click();
      const form = page.getByRole("dialog", { name: /Expense record/ });
      await expect(form).toBeVisible();

      // The fields a person fills in come first, in this order, and nothing is above them.
      const labels = await form.locator(".flabel").allInnerTexts();
      expect(labels.slice(0, 4).map((l) => l.replace(/\s*\*$/, "").trim())).toEqual([
        "Description",
        "Amount",
        "Category",
        "Date",
      ]);
      await form.getByLabel("Description").fill(what);
      await form.getByRole("textbox", { name: "Amount" }).fill("38.50");
      await form.getByRole("combobox", { name: /^Category/ }).selectOption({ label: "Travel" });
      await form.getByRole("button", { name: "Create" }).click();
      await expect(page.locator(".toast").filter({ hasText: "Expense created" })).toBeVisible();

      // Once saved the receipt can be added, in the same open group.
      await form.locator('input[type="file"]').first().setInputFiles({
        name: "receipt.png",
        mimeType: "image/png",
        buffer: png,
      });
      await form.getByRole("button", { name: "Submit", exact: true }).click();
      await expect(
        page.locator(".toast").filter({ hasText: "Submitted for approval" }),
      ).toBeVisible();

      const claim = (
        await api.run("root", "expenses_expense.search", {
          filter: [{ field: "description", op: "eq", value: what }],
        })
      ).rows[0];
      expect(claim.state).toBe("submitted");
      expect(claim.amount).toBe(3850);
      expect(claim.category).toBe("travel");
      expect(claim.employee).toBe("root");
      expect(claim.receipt, "the receipt was stored").toBeTruthy();
      const wide = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
      );
      expect(wide, "no sideways scroll").toBe(false);
    });
  });
});

test.describe("Expenses: the receipt on a touch phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  test("the Download button of the receipt is as tall as the buttons beside it", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await withExpenses(api, async () => {
      const claim = await api.create("root", "expenses_expense", {
        description: uid("Hotel"),
        amount: 12000,
        category: "travel",
      });
      await signIn("root", `/expenses/expenses_expense?view=list&id=${claim.id}`);
      const field = page.locator(".blobfield").first();
      await field.locator('input[type="file"]').setInputFiles({
        name: "receipt.png",
        mimeType: "image/png",
        buffer: png,
      });
      const download = field.locator(".blobrow .filechip");
      await expect(download).toBeVisible();
      const h = await download.evaluate((e) => Math.round(e.getBoundingClientRect().height));
      expect(h, "a finger-sized Download button").toBeGreaterThanOrEqual(44);
    });
  });
});
