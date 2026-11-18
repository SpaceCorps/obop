import { expect, test } from "../support/fixtures.ts";

// A filter a package declares is shown in the language of the back office (docs/ISSUES.md U10):
// hr translates "Not refused", the filter its Time off list opens with, in its label files.

test.describe("in German", () => {
  test.use({ language: "de" });
  test("the filter hr declares is shown in German (U10)", async ({ page, signIn, api }) => {
    const root = await api.token("root");
    const had = ((await api.run("root", "module.list")) as any[]).some((m) => m.id === "hr");
    if (!had) await api.run("root", "marketplace_listing.install", { id: "hr" });
    try {
      await signIn("root", "/hr/hr_leave_request?view=list");
      await expect(page.locator(".facet")).toHaveText(/Nicht abgelehnt/);
    } finally {
      if (!had) await api.post("/api/module/uninstall", { id: "hr" }, root);
    }
  });
});
