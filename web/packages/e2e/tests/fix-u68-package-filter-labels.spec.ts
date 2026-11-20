import { expect, test } from "../support/fixtures.ts";

// The quick filters a package declares are shown in the language of the back office
// (docs/ISSUES.md U52, U68): Documents translates "Drafts" and "Confidential" in its label files,
// beside the back office's own filters.

test.describe("in German", () => {
  test.use({ language: "de" });
  test("the filters Documents declares are shown in German (U52, U68)", async ({
    page,
    signIn,
    api,
  }) => {
    const root = await api.token("root");
    const had = ((await api.run("root", "module.list")) as any[]).some((m) => m.id === "documents");
    if (!had) await api.run("root", "marketplace_listing.install", { id: "documents" });
    try {
      await signIn("root", "/documents/documents_document?view=list");
      // The app menu lists the package's filters as links.
      const menu = page.getByRole("navigation", { name: "Menü Dokumente" });
      await expect(menu.getByRole("link", { name: "Entwürfe", exact: true })).toBeVisible();
      await expect(menu.getByRole("link", { name: "Vertraulich", exact: true })).toBeVisible();
      await expect(menu.getByRole("link", { name: "Drafts", exact: true })).toHaveCount(0);
      await expect(menu.getByRole("link", { name: "Confidential", exact: true })).toHaveCount(0);
    } finally {
      if (!had) await api.post("/api/module/uninstall", { id: "documents" }, root);
    }
  });
});
