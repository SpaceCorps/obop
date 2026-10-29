import type { Page } from "@playwright/test";
import { coreVersion } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// D90: limits on fields. A throwaway package declares a headline of at most 2,000 characters, a
// short code of at most 10, a quantity from 0 to 100 and a rate from 0.10 to 9.99 (a decimal, so
// exact). The form counts characters near the limit, refuses a value outside it before sending
// and, when the browser could not know, shows the server's own refusal.

const q = JSON.stringify;

// A package that uses limits declares the core that has them (D90: ^0.2.1 and up).
const manifest = (id: string) => `
[module]
id = ${q(id)}
version = "0.1.0"
name = "Limits"
description = "Fields with limits (end-to-end test module)"
author = "obop e2e"
requires_core = ${q(`^${coreVersion}`)}

[[model]]
name = "${id}_note"
description = "A note"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "headline"
  description = "A headline"
  class = "none"
  max_length = 2000
  min_length = 1

  [[model.field]]
  name = "code"
  description = "A short code"
  class = "none"
  max_length = 10

  [[model.field]]
  name = "details"
  description = "Details, with the default length of a long text"
  class = "none"
  kind = "longtext"

  [[model.field]]
  name = "qty"
  description = "A quantity"
  class = "none"
  kind = "number"
  min = 0
  max = 100

  [[model.field]]
  name = "rate"
  description = "A rate"
  class = "none"
  kind = "decimal"
  precision = 8
  scale = 2
  min = "0.10"
  max = "9.99"
`;

const panel = (page: Page) => page.getByRole("complementary").first();
const counter = (page: Page) => panel(page).locator(".hint.counter");

test.describe("field limits", () => {
  test("a text near its limit shows a counter, and a text over it is refused before it is sent", async ({
    page,
    signIn,
    api,
    scratchPackage,
    uid,
  }) => {
    const id = await scratchPackage(manifest);
    const model = `${id}_note`;
    await signIn("root", `/${id}/${model}?view=list&id=new`);
    const code = panel(page).getByLabel("Code");

    // Far from the limit nothing is shown: a counter on every field would be noise.
    await code.fill("abc");
    await expect(counter(page)).toHaveCount(0);
    // At four fifths it appears, and counts.
    await code.fill("abcdefgh");
    await expect(counter(page)).toHaveText("8 of 10 characters");
    await expect(counter(page)).not.toHaveClass(/over/);
    // The control is not cut at the limit (a pasted text would lose its end without a word):
    // it goes red, and the form says what is wrong.
    await code.fill("abcdefghijk");
    await expect(counter(page)).toHaveText("11 of 10 characters");
    await expect(counter(page)).toHaveClass(/over/);
    await expect(code).not.toHaveAttribute("maxlength", /.*/);
    await panel(page).getByLabel("Headline").fill(uid("Over"));
    await panel(page).getByRole("button", { name: "Create" }).click();
    await expect(
      panel(page)
        .getByRole("alert")
        .filter({ hasText: "Code must be at most 10 characters; this has 11" }),
    ).toBeVisible();
    await expect(code).toHaveAttribute("aria-invalid", "true");
    expect((await api.list("root", model)).length).toBe(0);

    // Inside the limit it saves. Ten letters with a diaeresis are ten characters, not twenty bytes.
    await code.fill("ä".repeat(10));
    await panel(page).getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    const saved = await api.list("root", model);
    expect(saved.length).toBe(1);
    expect(saved[0].code).toBe("ä".repeat(10));
  });

  test("2,001 characters in a field of 2,000 are refused, and counted as characters", async ({
    page,
    signIn,
    api,
    scratchPackage,
  }) => {
    const id = await scratchPackage(manifest);
    const model = `${id}_note`;
    await signIn("root", `/${id}/${model}?view=list&id=new`);
    const headline = panel(page).getByLabel("Headline");
    await headline.fill("x".repeat(2001));
    await expect(counter(page)).toHaveText("2,001 of 2,000 characters");
    await expect(counter(page)).toHaveClass(/over/);
    await panel(page).getByRole("button", { name: "Create" }).click();
    await expect(
      panel(page)
        .getByRole("alert")
        .filter({ hasText: "Headline must be at most 2,000 characters; this has 2,001" }),
    ).toBeVisible();
    // 2,000 emoji are 4,000 UTF-16 units and 8,000 bytes, and 2,000 characters.
    await headline.fill("\u{1F600}".repeat(2000));
    await expect(counter(page)).toHaveText("2,000 of 2,000 characters");
    await expect(counter(page)).not.toHaveClass(/over/);
    await panel(page).getByRole("button", { name: "Create" }).click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    expect((await api.list("root", model))[0].headline).toBe("\u{1F600}".repeat(2000));
  });

  test("a number and a decimal outside their range are refused, exactly", async ({
    page,
    signIn,
    api,
    scratchPackage,
    uid,
  }) => {
    const id = await scratchPackage(manifest);
    const model = `${id}_note`;
    await signIn("root", `/${id}/${model}?view=list&id=new`);
    await panel(page).getByLabel("Headline").fill(uid("Range"));
    const create = panel(page).getByRole("button", { name: "Create" });
    const alert = (text: string) => panel(page).getByRole("alert").filter({ hasText: text });

    await panel(page).getByLabel("Qty").fill("101");
    await panel(page).getByLabel("Rate").fill("0.10");
    await create.click();
    await expect(alert("Qty must be at most 100; this is 101")).toBeVisible();
    await panel(page).getByLabel("Qty").fill("-1");
    await create.click();
    await expect(alert("Qty must be at least 0; this is -1")).toBeVisible();
    await panel(page).getByLabel("Qty").fill("100");
    await panel(page).getByLabel("Rate").fill("0.09");
    await create.click();
    await expect(alert("Rate must be at least 0.10; this is 0.09")).toBeVisible();
    await panel(page).getByLabel("Rate").fill("10.00");
    await create.click();
    await expect(alert("Rate must be at most 9.99; this is 10.00")).toBeVisible();
    expect((await api.list("root", model)).length).toBe(0);

    // The ends are in.
    await panel(page).getByLabel("Rate").fill("9.99");
    await create.click();
    await expect(page.locator(".toast").filter({ hasText: "created" })).toBeVisible();
    const row = (await api.list("root", model))[0];
    expect([row.qty, row.rate]).toEqual([100, "9.99"]);
  });

  test("the server's refusal is shown when the browser did not know the limit", async ({
    page,
    signIn,
    api,
    scratchPackage,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const id = await scratchPackage(manifest);
    const model = `${id}_note`;
    // A page loaded before the limit was lowered: its copy of /api/meta has no limit on the code.
    await page.route("**/api/meta", async (route) => {
      const res = await route.fetch();
      const body = await res.json();
      for (const m of body.modules)
        for (const d of m.models)
          for (const f of d.fields) if (f.name === "code") delete f.max_length;
      await route.fulfill({ response: res, json: body });
    });
    await signIn("root", `/${id}/${model}?view=list&id=new`);
    await panel(page).getByLabel("Headline").fill(uid("Stale"));
    await panel(page).getByLabel("Code").fill("abcdefghijk");
    await expect(counter(page)).toHaveCount(0);
    await panel(page).getByRole("button", { name: "Create" }).click();
    // The server's sentence, on the field, in the words it sent.
    await expect(
      panel(page)
        .getByRole("alert")
        .filter({ hasText: `${model}.code must be at most 10 characters; this has 11` }),
    ).toBeVisible();
    expect((await api.list("root", model)).length).toBe(0);
  });

  test("an import names the row and the limit", async ({
    page,
    signIn,
    api,
    scratchPackage,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const id = await scratchPackage(manifest);
    const model = `${id}_note`;
    await signIn("root", `/${id}/${model}?view=list`);
    await page.getByRole("button", { name: "Import", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: /^Import .* from a CSV file$/ });
    await dialog.getByLabel("Choose a CSV file").setInputFiles({
      name: "notes.csv",
      mimeType: "text/csv",
      buffer: Buffer.from(
        `headline,code,qty\nfine,abc,5\nfine,${"c".repeat(11)},5\nfine,abc,101\n`,
      ),
    });
    const problems = dialog.getByRole("table", { name: "Problems found" });
    await expect(problems).toContainText("must be at most 10 characters; this has 11");
    await expect(problems).toContainText("must be at most 100; this is 101");
    expect((await api.list("root", model)).length).toBe(0);
  });
});
