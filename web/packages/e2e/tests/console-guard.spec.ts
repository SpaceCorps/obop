import { expect, test } from "../support/fixtures.ts";

// The console guard fails a test on any unexpected 4xx or 5xx answer a page provoked. The one
// exception: after the test itself uninstalled a module (the try/finally of the specs that install a
// package), a page that still refreshes asks for a model that is gone and is answered 404. That is
// not a fault of the page, and it made specs fail only when the refresh landed late (CI).

test("a request of the page answered 404 after the test uninstalled its module is not a console error", async ({
  page,
  api,
  signIn,
}) => {
  const root = await api.token("root");
  const r = await api.post("/api/marketplace_listing/install", { id: "subscriptions" }, root);
  expect(r.status, JSON.stringify(r.json)).toBe(200);
  await signIn("root", "/subscriptions/subscriptions_subscription?view=list");
  await api.post("/api/module/uninstall", { id: "subscriptions" }, root);
  const status = await page.evaluate(async () => {
    const res = await fetch("/api/subscriptions_subscription/search", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${sessionStorage.getItem("obop-token")}`,
      },
      body: "{}",
    });
    return res.status;
  });
  expect(status).toBe(404);
});
