import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { Page } from "@playwright/test";
import { repoRoot } from "../support/env.ts";
import { expect, open, test, unnamedControls, type Api } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

// D55: the Modules page. Every module declares its version, licence, core requirement and
// dependencies with version ranges; the page shows all of it and installs a module together with
// what it needs, as one confirmed plan.

const MODULES = "/customize/modules?tab=modules";

/** A small valid package; `extra` lands in the [module] table. */
function pkg(id: string, version: string, extra = "", deps = ""): string {
  return `[module]
id = "${id}"
version = "${version}"
name = "${id}"
description = "Test package ${id}"
summary = "Summary of ${id}"
author = "E2E"
license = "Apache-2.0"
requires_core = "^0.2"
${extra}
${deps}
[[model]]
name = "${id}_item"
description = "An item"
read_group = "${id}.user"
write_group = "${id}.user"

  [[model.field]]
  name = "title"
  description = "Title"
  class = "none"
`;
}

const card = (page: Page, name: string) => page.getByRole("article", { name, exact: true });
const toast = (page: Page, text: string) => page.locator(".toast").filter({ hasText: text });

async function installed(api: Api): Promise<string[]> {
  return (await api.run("root", "module.list")).map((m: { id: string }) => m.id);
}

/** Remove timesheets before projects (the dependent first); harmless when they are not installed. */
async function clean(api: Api, ...ids: string[]) {
  const have = await installed(api);
  for (const id of ids) if (have.includes(id)) await api.run("root", "module.uninstall", { id });
}

test.describe("Customize > Modules", () => {
  test("cards show version, author, licence, maturity, core compatibility, source and dependencies", async ({
    page,
    signIn,
  }) => {
    await signIn("root", MODULES);
    await expect(page.getByRole("heading", { name: "Modules", level: 2 })).toBeVisible();
    // The header names the core the instance runs and what that means.
    await expect(page.getByText("Core 0.2.0", { exact: true }).first()).toBeVisible();
    await expect(page.getByText(/every module runs on/)).toBeVisible();

    // A built-in module has the same facts as a package.
    const contacts = card(page, "Contacts");
    await expect(contacts).toContainText("0.2.0 · SpaceCorps");
    await expect(contacts).toContainText("AGPL-3.0-or-later");
    await expect(contacts).toContainText("Stable");
    await expect(contacts).toContainText("Works with core 0.2");
    await expect(contacts).toContainText("Built in");
    await expect(contacts).not.toContainText("Needs:");
    await expect(contacts).toContainText("Required by:");
    await expect(contacts.getByRole("button")).toHaveCount(0);
    // Sales needs Contacts and Products, and says so in names.
    await expect(card(page, "Sales")).toContainText("Needs: Contacts, Products");
    await expect(card(page, "Contacts")).toContainText("Required by:");

    await page.getByRole("tab", { name: /^Marketplace/ }).click();
    const ts = card(page, "Timesheets");
    await expect(ts).toBeVisible();
    await expect(ts).toContainText("0.3.0 · SpaceCorps");
    await expect(ts).toContainText("Beta");
    await expect(ts).toContainText("Works with core 0.2");
    await expect(ts).toContainText("Marketplace");
    await expect(ts).toContainText("Needs: Projects");
    await expect(ts.getByRole("button", { name: "Install" })).toBeEnabled();
    await expect(card(page, "Field Service")).toContainText(
      "Needs: Contacts, Human Resources, Products",
    );
    await expect(page.getByRole("tab", { name: /^Marketplace/ })).toContainText("22");
  });

  test("search, category and sort narrow the list", async ({ page, signIn }) => {
    await signIn("root", `${MODULES}&view=marketplace`);
    await expect(card(page, "Timesheets")).toBeVisible();
    await page.getByRole("textbox", { name: "Search modules" }).fill("time-tracking");
    await expect(page.getByRole("article")).toHaveCount(1);
    await expect(card(page, "Timesheets")).toBeVisible();
    await page.getByRole("textbox", { name: "Search modules" }).fill("no such module anywhere");
    await expect(page.getByText("No module matches")).toBeVisible();
    await page.getByRole("button", { name: "Clear filters" }).click();
    await expect(card(page, "Timesheets")).toBeVisible();

    await page.getByLabel("Category").selectOption({ label: "Finance" });
    await expect(card(page, "Accounting")).toBeVisible();
    await expect(card(page, "Subscriptions")).toBeVisible();
    await expect(card(page, "Fleet")).toHaveCount(0);
    await page.getByLabel("Category").selectOption({ label: "All categories" });

    await page.getByLabel("Sort by").selectOption({ label: "Name" });
    const names = await page
      .getByRole("article")
      .evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")!));
    expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b)));
  });

  test("installing timesheets shows a plan that includes projects, then installs both with one confirmation", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await signIn("root", `${MODULES}&view=marketplace`);
      await card(page, "Timesheets").getByRole("button", { name: "Install" }).click();
      const dialog = page.getByRole("dialog", { name: "Install Timesheets" });
      await expect(dialog).toBeVisible();
      // The plan: in order, with versions and licences, nothing changed yet.
      const steps = dialog
        .getByRole("list", { name: "Steps, in the order they run" })
        .locator(":scope > li");
      await expect(steps).toHaveCount(2);
      await expect(steps.nth(0)).toContainText("Projects");
      await expect(steps.nth(0)).toContainText("0.4.0");
      await expect(steps.nth(0)).toContainText("AGPL-3.0-or-later");
      await expect(steps.nth(0)).toContainText("needed by timesheets");
      await expect(steps.nth(1)).toContainText("Timesheets");
      await expect(steps.nth(1)).toContainText("The module you asked for.");
      expect(await installed(api)).not.toContain("projects");
      await dialog.getByRole("button", { name: "Install 2 modules" }).click();
      await expect(toast(page, "Installed Timesheets")).toBeVisible();
      expect(await installed(api)).toEqual(expect.arrayContaining(["projects", "timesheets"]));
      // The card now says so, and the Installed list has both.
      await expect(
        card(page, "Timesheets").getByRole("button", { name: "Installed" }),
      ).toBeDisabled();
      await page.getByRole("tab", { name: /^Installed/ }).click();
      await expect(card(page, "Projects")).toContainText("Required by: Timesheets");
      await expect(card(page, "Timesheets")).toContainText("Needs: Projects");
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test("a module that others need cannot be uninstalled, and the dialog names who needs it", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await api.run("root", "marketplace_listing.install", { id: "timesheets" });
      await signIn("root", MODULES);
      await card(page, "Projects").getByRole("button", { name: "Uninstall" }).click();
      const dialog = page.getByRole("alertdialog", { name: "Uninstall Projects?" });
      await expect(dialog).toContainText("Other modules need Projects");
      await expect(dialog.getByRole("listitem")).toContainText("Timesheets");
      await expect(dialog.getByRole("button", { name: "Uninstall" })).toHaveCount(0);
      await dialog.getByRole("button", { name: "Close" }).first().click();
      expect(await installed(api)).toContain("projects");

      // Remove the dependent first and then it can go.
      await card(page, "Timesheets").getByRole("button", { name: "Uninstall" }).click();
      await page.getByRole("alertdialog").getByRole("button", { name: "Uninstall" }).click();
      await expect(toast(page, "Removed Timesheets")).toBeVisible();
      await card(page, "Projects").getByRole("button", { name: "Uninstall" }).click();
      await page.getByRole("alertdialog").getByRole("button", { name: "Uninstall" }).click();
      await expect(toast(page, "Removed Projects")).toBeVisible();
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test("an upgrade that would break an installed dependent is explained before anything happens", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await api.run("root", "marketplace_listing.install", { id: "timesheets" });
      await signIn("root", `${MODULES}&view=marketplace`);
      await page.getByRole("button", { name: "Paste the text instead" }).click();
      await page.getByLabel("Text of the obop.toml").fill(pkg("projects", "2.0.0"));
      const check = page.locator(".mods-check");
      await expect(check.getByText("This package cannot be installed here")).toBeVisible();
      await expect(check).toContainText(
        "upgrading projects to 2.0.0 would break timesheets, which needs projects >=0.3, <1",
      );
      await expect(check.getByText("Conflict", { exact: true })).toBeVisible();
      await expect(check.getByRole("button", { name: "Install" })).toBeDisabled();
      // A newer version inside the range is an upgrade and can go ahead.
      await page.getByLabel("Text of the obop.toml").fill(pkg("projects", "0.9.0"));
      await expect(check.getByText("This is a valid package")).toBeVisible();
      await expect(check.getByText("Upgrade", { exact: true })).toBeVisible();
      await expect(check.getByRole("button", { name: "Install" })).toBeEnabled();
      expect(
        (await api.run("root", "module.list")).find((m: { id: string }) => m.id === "projects")
          .version,
      ).toBe("0.4.0");
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test("a package that needs a newer core is refused with both versions", async ({
    page,
    signIn,
    api,
  }) => {
    await signIn("root", `${MODULES}&view=marketplace`);
    await page.getByRole("button", { name: "Paste the text instead" }).click();
    await page
      .getByLabel("Text of the obop.toml")
      .fill(pkg("futurepack", "1.0.0").replace('requires_core = "^0.2"', 'requires_core = ">=9"'));
    const check = page.locator(".mods-check");
    await expect(check.getByText("This package cannot be installed here")).toBeVisible();
    await expect(check).toContainText("futurepack 1.0.0 needs core >=9, you run 0.2.0");
    await expect(check.getByRole("button", { name: "Install" })).toBeDisabled();
    // The server refuses it too, whatever the page says.
    const r = await api.post(
      "/api/module/install",
      {
        manifest: pkg("futurepack", "1.0.0").replace(
          'requires_core = "^0.2"',
          'requires_core = ">=9"',
        ),
      },
      await api.token("root"),
    );
    expect(r.status).toBe(400);
    expect(r.json.error).toBe("futurepack 1.0.0 needs core >=9, you run 0.2.0");
    expect(await installed(api)).not.toContain("futurepack");
  });

  test("a file with a mistake shows the parse error before anything is installed", async ({
    page,
    signIn,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await signIn("root", `${MODULES}&view=marketplace`);
    await page.getByLabel("Upload an obop.toml file").setInputFiles({
      name: "broken.toml",
      mimeType: "text/plain",
      buffer: Buffer.from(pkg("broken", "1.0").replace("Apache-2.0", "Apache")),
    });
    await expect(page.getByText("This package cannot be read")).toBeVisible();
    await expect(page.locator(".mods-check")).toContainText("version must look like 1.2.3");
  });

  test("an uploaded package is checked, shown as a plan, installed through the plan dialog and removed", async ({
    page,
    signIn,
    api,
  }) => {
    const id = `up${Date.now().toString(36)}`;
    try {
      await signIn("root", `${MODULES}&view=marketplace`);
      await page.getByLabel("Upload an obop.toml file").setInputFiles({
        name: `${id}.toml`,
        mimeType: "text/plain",
        buffer: Buffer.from(
          pkg(
            id,
            "0.1.0",
            'keywords = ["upload"]',
            '[module.dependencies]\nprojects = ">=0.3, <1"',
          ),
        ),
      });
      const check = page.locator(".mods-check");
      await expect(check.getByText(`${id}.toml is a valid package`)).toBeVisible();
      // Its dependency comes from the marketplace, first.
      const steps = check
        .getByRole("list", { name: "Steps, in the order they run" })
        .locator(":scope > li");
      await expect(steps).toHaveCount(2);
      await expect(steps.nth(0)).toContainText("Projects");
      await check.getByRole("button", { name: "Install" }).click();
      const dialog = page.getByRole("dialog", { name: "Install this package" });
      await dialog.getByRole("button", { name: "Install 2 modules" }).click();
      await expect(toast(page, `Installed ${id}`)).toBeVisible();
      expect(await installed(api)).toEqual(expect.arrayContaining(["projects", id]));
      await page.getByRole("tab", { name: /^Installed/ }).click();
      await expect(card(page, id)).toContainText("Upload");
      await expect(card(page, id)).toContainText("Apache-2.0");
    } finally {
      await clean(api, id, "projects");
    }
  });

  test("the detail panel shows the dependency tree with required ranges, installed versions and status", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await api.run("root", "marketplace_listing.install", { id: "field_service" });
      await signIn("root", `${MODULES}&id=field_service`);
      const panel = page.getByRole("complementary", { name: "Field Service module" });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole("heading", { name: "Field Service" })).toBeVisible();
      // Overview: the metadata table.
      await expect(panel).toContainText("AGPL-3.0-or-later");
      await expect(panel.getByRole("link", { name: /AGPL-3.0-or-later/ })).toHaveAttribute(
        "href",
        "https://spdx.org/licenses/AGPL-3.0-or-later.html",
      );
      await expect(
        panel.getByRole("link", { name: /github.com\/SpaceCorps\/obop/ }).first(),
      ).toHaveAttribute("rel", /noopener/);
      await expect(panel).toContainText("SpaceCorps");
      await expect(panel).toContainText("field-service");

      expect(await unnamedControls(page), "module panel").toEqual([]);
      await panel.getByRole("tab", { name: "Dependencies" }).click();
      await expect(page).toHaveURL(/section=dependencies/);
      await expect(panel.getByText("Field Service needs core ^0.2")).toBeVisible();
      await expect(panel.getByText("you run 0.2.0").first()).toBeVisible();
      const tree = panel.getByRole("list", { name: "Dependencies of Field Service" }).first();
      const hr = tree.getByRole("listitem").filter({ hasText: "Human Resources" }).first();
      await expect(hr).toContainText("≥0.2, <1");
      await expect(hr).toContainText("installed 0.3.0");
      await expect(hr).toContainText("Satisfied");
      const base = tree.getByRole("listitem").filter({ hasText: "Contacts" }).first();
      await expect(base).toContainText("Built in");
      await expect(base).toContainText("installed 0.2.0");
      // A module in the tree opens its own panel; Required by lists the dependents.
      await hr.getByRole("link", { name: "Human Resources" }).click();
      await expect(page).toHaveURL(/id=hr/);
      const hrPanel = page.getByRole("complementary", { name: "Human Resources module" });
      await expect(hrPanel).toBeVisible();
      await expect(hrPanel.getByText("Required by")).toBeVisible();
      await expect(hrPanel.getByRole("link", { name: "Field Service" })).toBeVisible();

      await hrPanel.getByRole("tab", { name: "Contents" }).click();
      await expect(hrPanel).toContainText("hr_employee");
      await expect(hrPanel).toContainText("Holds personal data");
      await expect(hrPanel).toContainText("Permission groups");
      await hrPanel.getByRole("button", { name: "Close module" }).click();
      await expect(hrPanel).toHaveCount(0);
      await expect(page).not.toHaveURL(/id=hr/);
    } finally {
      await clean(api, "field_service", "hr");
    }
  });

  test("the instance overview lists every dependency as a table too", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await api.run("root", "marketplace_listing.install", { id: "timesheets" });
      await signIn("root", MODULES);
      const overview = page.getByRole("img", {
        name: "Dependencies between the installed modules",
      });
      await expect(overview).toBeVisible();
      await page.getByText("Show the same as a table").click();
      expect(await unnamedControls(page), "overview").toEqual([]);
      const row = page.getByRole("row", { name: /^Timesheets Projects/ });
      await expect(row).toContainText("≥0.3, <1");
      await expect(page.getByRole("row", { name: /^Projects Nothing Timesheets/ })).toBeVisible();
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test("Modules is reachable from the command palette and the address keeps the open module", async ({
    page,
    signIn,
  }) => {
    await signIn("root");
    await page.keyboard.press("ControlOrMeta+k");
    const search = page
      .getByRole("textbox", { name: "Search apps, records and actions" })
      .or(page.getByRole("combobox", { name: "Search apps, records and actions" }));
    await search.fill("modules");
    await page
      .getByRole("option", { name: /^Modules/ })
      .first()
      .click();
    await expect(page).toHaveURL(/#\/customize\/modules/);
    await page.getByRole("link", { name: "Contacts", exact: true }).first().click();
    await expect(page).toHaveURL(/id=base/);
    await expect(page.getByRole("complementary", { name: "Contacts module" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("complementary", { name: "Contacts module" })).toHaveCount(0);

    await page.keyboard.press("ControlOrMeta+k");
    await search.fill("marketplace");
    await page.getByRole("option", { name: /Install a module from the marketplace/ }).click();
    await expect(page.getByRole("tab", { name: /^Marketplace/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("an agent proposes the install of timesheets and one approval installs both modules", async ({
    api,
  }) => {
    const root = await api.token("root");
    try {
      const t = await api.post(
        "/api/auth/agent-token",
        { scopes: ["platform.admin"], label: "mods" },
        root,
      );
      const agent = t.json.token as string;
      const plan = await api.post("/api/module_plan/preview", { id: "timesheets" }, agent);
      expect(plan.status).toBe(200);
      expect(plan.json.value.steps.map((s: { id: string }) => s.id)).toEqual([
        "projects",
        "timesheets",
      ]);
      const proposed = await api.post(
        "/api/marketplace_listing/install",
        { id: "timesheets" },
        agent,
      );
      expect(proposed.status).toBe(202);
      expect(await installed(api)).not.toContain("projects");
      const done = await api.post(`/api/approve/${proposed.json.plan_id}`, {}, root);
      expect(done.status).toBe(200);
      expect(done.json.value.steps).toHaveLength(2);
      expect(await installed(api)).toEqual(expect.arrayContaining(["projects", "timesheets"]));
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test("the page and its panel have no sideways scroll or cut-off controls on a phone", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await api.run("root", "marketplace_listing.install", { id: "timesheets" });
      await page.setViewportSize({ width: 390, height: 844 });
      await signIn("root", MODULES);
      await expect(card(page, "Contacts")).toBeVisible();
      expect(await layoutProblems(page), "installed").toEqual([]);
      await page.getByRole("tab", { name: /^Marketplace/ }).click();
      await expect(card(page, "Timesheets")).toBeVisible();
      expect(await layoutProblems(page), "marketplace").toEqual([]);
      await open(page, `${MODULES}&id=timesheets&section=dependencies`);
      await expect(page.getByRole("complementary", { name: "Timesheets module" })).toBeVisible();
      expect(await layoutProblems(page), "panel").toEqual([]);
      await page.getByRole("button", { name: "Close module" }).click();
      await open(page, `${MODULES}&view=marketplace`);
      await card(page, "Subscriptions").getByRole("button", { name: "Install" }).click();
      await expect(
        page
          .getByRole("dialog", { name: "Install Subscriptions" })
          .getByRole("list", { name: /Steps/ }),
      ).toBeVisible();
      expect(await layoutProblems(page), "plan dialog").toEqual([]);
    } finally {
      await clean(api, "timesheets", "projects");
    }
  });

  test.describe("in German", () => {
    test.use({ language: "de" });
    test("the page renders in German, with the module names the server translates", async ({
      page,
      signIn,
      api,
    }) => {
      const de: Record<string, string> = JSON.parse(
        readFileSync(resolve(repoRoot, "web/apps/backoffice/locales/de/messages.json"), "utf8"),
      );
      const t = (id: string) => {
        const v = de[id];
        if (!v) throw new Error(`no de translation of ${id}`);
        return v;
      };
      try {
        await api.run("root", "marketplace_listing.install", { id: "timesheets" });
        await page.setViewportSize({ width: 390, height: 844 });
        await signIn("root", MODULES);
        await expect(page.getByRole("heading", { name: t("Modules"), level: 2 })).toBeVisible();
        await expect(
          page.getByRole("tab", { name: new RegExp(`^${t("Installed")}`) }),
        ).toBeVisible();
        await expect(page.getByRole("button", { name: t("Uninstall") }).first()).toBeVisible();
        await expect(page.getByText(t("Part of the core")).first()).toBeVisible();
        expect(await layoutProblems(page), "de installed").toEqual([]);
        await page.getByRole("tab", { name: new RegExp(`^${t("Marketplace")}`) }).click();
        await expect(page.getByLabel(t("Category"))).toBeVisible();
        expect(await layoutProblems(page), "de marketplace").toEqual([]);
        await page.setViewportSize({ width: 1440, height: 900 });
        await open(page, `${MODULES}&id=timesheets&section=dependencies`);
        await expect(page.getByRole("tab", { name: t("Dependencies") })).toBeVisible();
        await expect(page.getByRole("heading", { name: t("Required by") })).toBeVisible();
        expect(await layoutProblems(page), "de panel").toEqual([]);
      } finally {
        await clean(api, "timesheets", "projects");
      }
    });
  });
});
