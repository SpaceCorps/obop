import { expect, test, type Api } from "../support/fixtures.ts";

// Subscriptions (UI plan 3.3): a new subscription for a customer and a plan takes the plan's price,
// starts as a trial and is activated with the one primary action.

async function withSubscriptions(api: Api, run: () => Promise<void>) {
  const root = await api.token("root");
  const r = await api.post("/api/marketplace_listing/install", { id: "subscriptions" }, root);
  expect(r.status, JSON.stringify(r.json)).toBe(200);
  try {
    await run();
  } finally {
    await api.post("/api/module/uninstall", { id: "subscriptions" }, root);
  }
}

test("a subscription takes the plan's price and is activated", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  await withSubscriptions(api, async () => {
    const customer = await api.create("root", "party", { name: uid("Baumann Bau") });
    const plan = await api.create("root", "subscriptions_plan", {
      name: uid("Tool fleet monthly"),
      interval: "monthly",
      price: 19600,
    });
    await signIn("root", "/subscriptions/subscriptions_subscription?view=list&id=new");
    const form = page.locator(".record");
    await form.getByLabel("Customer").fill(customer.name);
    await page.getByRole("option", { name: new RegExp(customer.name) }).click();
    await form.getByLabel("Plan").fill(plan.name);
    await page.getByRole("option", { name: new RegExp(plan.name) }).click();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await expect(form.getByRole("button", { name: "Activate" })).toBeVisible();
    const sub = (await api.list("root", "subscriptions_subscription")).find(
      (s) => s.plan_id === plan.id,
    );
    expect(sub.recurring_amount).toBe(19600);
    expect(sub.state).toBe("trial");
    await form.getByRole("button", { name: "Activate" }).click();
    await expect
      .poll(
        async () => (await api.run("root", "subscriptions_subscription.get", { id: sub.id })).state,
      )
      .toBe("active");
  });
});
