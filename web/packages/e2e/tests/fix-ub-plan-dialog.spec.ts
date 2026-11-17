import type { Page } from "@playwright/test";
import { tr } from "../support/catalogue.ts";
import { expect, test, type Api } from "../support/fixtures.ts";

// The refusals of the install and the uninstall dialog are shown in the person's language
// (docs/ISSUES.md D15c): PlanDialog.tsx keeps the server's sentence through serverMessage, and in
// English the output is the same with or without it, so these run in German. Each refusal is
// provoked by changing the instance through the API while the dialog is open.

test.use({ language: "de" });

const MODULES = "/customize/modules?tab=modules";
const card = (page: Page, id: string) =>
  page.locator("article.mod-card").filter({ has: page.locator(`a[data-module="${id}"]`) });

/** The words of a translated message between its {placeholders}: what the notice must contain. */
const fragments = (template: string) =>
  template
    .split(/\{\w+\}/)
    .map((s) => s.trim())
    .filter((s) => s.length >= 3);

async function installed(api: Api): Promise<string[]> {
  return ((await api.run("root", "module.list")) as { id: string }[]).map((m) => m.id);
}

async function remove(api: Api, ...ids: string[]) {
  const root = await api.token("root");
  for (const id of ids)
    if ((await installed(api)).includes(id)) await api.post("/api/module/uninstall", { id }, root);
}

test.describe("refusals of the module dialogs in German (D15c)", () => {
  test.setTimeout(90_000);

  test("the install dialog says in German that the module is already installed", async ({
    page,
    signIn,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400); // the refused install
    await remove(api, "fleet");
    try {
      await signIn("root", `${MODULES}&view=marketplace`);
      await card(page, "fleet")
        .getByRole("button", { name: tr("de", "Install"), exact: true })
        .click();
      const dialog = page.getByRole("dialog");
      const confirm = dialog.getByRole("button").last();
      await expect(confirm).toBeEnabled();
      // Meanwhile someone else installs it.
      await api.run("root", "marketplace_listing.install", { id: "fleet" });
      await confirm.click();
      const notice = dialog.getByText(tr("de", "The install failed"));
      await expect(notice).toBeVisible();
      const body = await dialog.innerText();
      for (const words of fragments(tr("de", "{module} {version} is already installed")))
        expect(body).toContain(words);
      expect(body).not.toContain("is already installed");
    } finally {
      await remove(api, "fleet");
    }
  });

  test("the uninstall dialog says in German who still needs the module", async ({
    page,
    signIn,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400); // the refused uninstall
    await remove(api, "timesheets");
    const had = (await installed(api)).includes("projects");
    if (!had) await api.run("root", "marketplace_listing.install", { id: "projects" });
    try {
      await signIn("root", MODULES);
      await card(page, "projects")
        .getByRole("button", { name: tr("de", "Uninstall"), exact: true })
        .click();
      const dialog = page.getByRole("alertdialog");
      const uninstall = dialog.getByRole("button", { name: tr("de", "Uninstall"), exact: true });
      await expect(uninstall).toBeVisible();
      // Meanwhile someone else installs a module that needs it.
      await api.run("root", "marketplace_listing.install", { id: "timesheets" });
      await uninstall.click();
      await expect(dialog.getByText(tr("de", "The removal failed"))).toBeVisible();
      const body = await dialog.innerText();
      for (const words of fragments(
        tr("de", "{module} is required by {others}; uninstall those first"),
      ))
        expect(body).toContain(words);
      expect(body).not.toContain("uninstall those first");
      expect(await installed(api)).toContain("projects");
    } finally {
      await remove(api, "timesheets");
      if (!had) await remove(api, "projects");
    }
  });
});
