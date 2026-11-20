import { expect, test, type Api } from "../support/fixtures.ts";

// Q14 (docs/ISSUES.md): one idea is drawn and named the same way in the finance and supply apps.
// - An action an agent may only propose carries the shield on a board card too, as it does in the
//   record panel (Expenses cards wrote "Approve *").
// - An app whose menu shows Overview opens on it: Purchase and Manufacturing listed Overview but
//   opened on their orders, while Accounting and Inventory opened on Overview.

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  const added = ids.filter((id) => !had.has(id));
  for (const id of added) await api.post("/api/marketplace_listing/install", { id }, root);
  try {
    await run();
  } finally {
    for (const id of [...added].reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

test.setTimeout(120_000);

test("a board card marks an action that needs approval with the shield", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  await withPackages(api, ["expenses"], async () => {
    const what = uid("Hotel for the fair");
    const e = await api.create("root", "expenses_expense", {
      description: what,
      amount: 12000,
      category: "lodging",
    });
    await api.run("root", "expenses_expense.submit", { id: e.id });
    await signIn("root", "/expenses/expenses_expense?view=kanban");
    const card = page.locator(".kcard, [data-card]").filter({ hasText: what }).first();
    await expect(card).toBeVisible();
    const next = card.locator("button.knext");
    await expect(next).toHaveAttribute("data-needs-approval", "true");
    await expect(next.locator(".approval-mark")).toHaveCount(1);
    expect((await next.innerText()).trim()).not.toMatch(/\*$/);
  });
});

test("Purchase and Manufacturing open on their Overview, like Accounting and Inventory", async ({
  page,
  signIn,
  api,
}) => {
  await withPackages(api, ["inventory", "purchase", "manufacturing"], async () => {
    for (const app of ["purchase", "manufacturing", "inventory"]) {
      await signIn("root", `/${app}`);
      await expect(page.locator('[aria-current="page"]').first()).toHaveText(/Overview/);
    }
  });
});
