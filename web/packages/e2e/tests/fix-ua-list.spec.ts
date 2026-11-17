import { expect, test } from "../support/fixtures.ts";

const leadRow = (page: import("@playwright/test").Page, title: string) =>
  page.getByRole("grid", { name: "Lead list" }).getByRole("row", { name: new RegExp(title) });

test.describe("shared list and record fixes (stream ua)", () => {
  test("a cell left by a click elsewhere saves what was typed; Escape still cancels (#87)", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const title = uid("Click away lead");
    await api.create("root", "lead", { title, contact_name: "Before" });
    await signIn("root", "/crm/lead?view=list");
    await page.getByRole("button", { name: "Edit in list" }).click();
    const cell = leadRow(page, title).locator('td[data-f="contact_name"]');
    await cell.click();
    await page.locator("td.editing input").fill("After click away");
    await page.getByText(/^\d+ records?$/).click();
    await expect(cell).toHaveText("After click away");
    await expect
      .poll(async () => (await api.list("root", "lead")).find((l) => l.title === title)?.contact_name)
      .toBe("After click away");

    await cell.click();
    await page.locator("td.editing input").fill("Never saved");
    await page.keyboard.press("Escape");
    await page.getByText(/^\d+ records?$/).click();
    await expect(cell).toHaveText("After click away");
    expect((await api.list("root", "lead")).find((l) => l.title === title)?.contact_name).toBe(
      "After click away",
    );
    await page.getByRole("button", { name: "Edit in list" }).click();
  });

  test("a picture column is not sortable: plain heading, not in the Sort menu (#89)", async ({
    page,
    signIn,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem("obop-list-cols:party", JSON.stringify(["name", "photo", "email"])),
    );
    await signIn("root", "/base/party?view=list");
    const head = page.getByRole("columnheader", { name: "Photo" });
    await expect(head).toBeVisible();
    await expect(head.getByRole("button")).toHaveCount(0);
    await expect(head).not.toHaveAttribute("aria-sort", /.*/);
    await expect(page.getByRole("columnheader", { name: /Name/ }).getByRole("button")).toBeVisible();
    // A phone sorts from a menu: the picture is not in it.
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole("button", { name: "Sort by" }).click();
    const menu = page.getByRole("group", { name: "Sort by" });
    await expect(menu.getByRole("button", { name: "Name" })).toBeVisible();
    await expect(menu.getByRole("button", { name: "Photo" })).toHaveCount(0);
  });

  test("the page never scrolls sideways while a record panel slides in (#86)", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    for (const n of ["A", "B"]) await api.create("root", "party", { name: uid(`Slide ${n}`) });
    await page.setViewportSize({ width: 1440, height: 900 });
    await signIn("root", "/base/party?view=list");
    await expect(page.locator("tbody tr").first()).toBeVisible();
    await page.evaluate(() => {
      const w = window as unknown as { maxWidth: number };
      w.maxWidth = 0;
      const tick = () => {
        w.maxWidth = Math.max(w.maxWidth, document.documentElement.scrollWidth);
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    for (let i = 0; i < 2; i++) {
      await page.locator("tbody tr").nth(i).locator("td").nth(1).click();
      await expect(page.locator("aside.record")).toBeVisible();
      await page.waitForTimeout(500);
      await page.keyboard.press("Escape");
      await expect(page.locator("aside.record")).toHaveCount(0);
    }
    const max = await page.evaluate(() => (window as unknown as { maxWidth: number }).maxWidth);
    expect(max).toBeLessThanOrEqual(1440);
  });
});

test("a person's list follows ui.order on a personal data field (R13)", async ({
  page,
  signIn,
}) => {
  const orders: string[] = [];
  page.on("request", (r) => {
    if (r.method() === "POST" && /\/api\/party\/search/.test(r.url()))
      orders.push(JSON.stringify(JSON.parse(r.postData() ?? "{}").order));
  });
  await signIn("root", "/base/party?view=list");
  await expect(page.locator("tbody tr").first()).toBeVisible();
  // Contacts ask for `name asc` (ui.order); the name is personal data a person reads in clear.
  await expect.poll(() => orders.at(-1)).toBe(JSON.stringify([{ field: "name", dir: "asc" }]));
});
