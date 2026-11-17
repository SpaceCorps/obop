import { expect, test, type Api } from "../support/fixtures.ts";

// The jobs board offers "Complete job" only once the customer signed (docs/ISSUES.md S4): a card of
// a job in progress that needs a signature shows no Complete job button, as the job page does not;
// once the sign-off is recorded the button is there. The server refuses the early complete either
// way (the `complete` assertion).

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  const added: string[] = [];
  try {
    for (const id of ids)
      if (!had.has(id)) {
        await api.run("root", "marketplace_listing.install", { id });
        added.push(id);
      }
    await run();
  } finally {
    for (const id of added.reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

test("a job card offers Complete job only after the sign-off (S4)", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  test.setTimeout(90_000);
  await withPackages(api, ["hr", "field_service"], async () => {
    const customer = await api.create("root", "party", { name: uid("Board GmbH") });
    const name = uid("Sign before done");
    const job = await api.create("root", "field_service_job", {
      name,
      customer_id: customer.id,
      signature_required: true,
    });
    await api.run("root", "field_service_job.depart", { id: job.id });
    await api.run("root", "field_service_job.start", { id: job.id });

    await signIn("root", "/field_service/field_service_job?view=kanban");
    const card = page.locator(".kcard", { hasText: name });
    await expect(card).toBeVisible();
    await expect(card.getByRole("button", { name: /^Complete/ })).toHaveCount(0);

    await api.run("root", "field_service_job.update", {
      id: job.id,
      signed_by: "Jens Moeller",
      signed_on: new Date().toISOString().slice(0, 10),
    });
    await page.reload();
    await expect(card.getByRole("button", { name: /^Complete/ })).toBeVisible();
  });
});
