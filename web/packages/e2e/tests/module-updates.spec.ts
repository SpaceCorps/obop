import type { Page } from "@playwright/test";
import { expect, test, type Api } from "../support/fixtures.ts";
import { field, offer, pkgText, withdraw } from "../support/market.ts";

// D63: updating a module from inside the instance. A newer version on the (local) marketplace shows
// its notes, what it changes in the data, which installed modules depend on it and the range each
// supports; it is applied as one confirmed operation, with the declared data steps, and can be rolled
// back while the data allows it. Dependency ranges are mandatory.

const MODULES = "/customize/modules?tab=modules";
const UPDATES = `${MODULES}&view=updates`;

const card = (page: Page, name: string) => page.getByRole("article", { name, exact: true });
const toast = (page: Page, text: string) => page.locator(".toast").filter({ hasText: text });

async function version(api: Api, id: string): Promise<string | undefined> {
  const rows = await api.run("root", "module.list");
  return rows.find((m: { id: string }) => m.id === id)?.version;
}

async function clean(api: Api, ...ids: string[]) {
  for (const id of ids) {
    withdraw(id);
    const have = (await api.run("root", "module.list")).map((m: { id: string }) => m.id);
    if (have.includes(id)) await api.run("root", "module.uninstall", { id });
  }
}

/** Marketplace uploads are modules too: an installed upload is updated from the marketplace. */
async function install(api: Api, text: string) {
  await api.run("root", "module.install", { manifest: text });
}

test.describe("Module updates", () => {
  test("a newer version shows its notes and data changes and updates with its declared steps, then rolls back", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await install(
        api,
        pkgText({ id: "e2eupa", version: "0.1.0", fields: field("plate"), changelog: "First." }),
      );
      await api.create("root", "e2eupa_item", { title: "first", plate: "AB-1" });
      await api.create("root", "e2eupa_item", { title: "second", plate: "CD-2" });
      offer({
        id: "e2eupa",
        version: "0.2.0",
        changelog: "Plates are now called registrations.\nEvery vehicle gets a tier.",
        fields: field("registration") + field("tier", "required = true"),
        migrations: `
[[migration]]
from = "<0.2.0"
op = "rename_field"
model = "e2eupa_item"
field = "plate"
to = "registration"

[[migration]]
from = "<0.2.0"
op = "set_default"
model = "e2eupa_item"
field = "tier"
value = "standard"
`,
      });

      await signIn("root", UPDATES);
      const c = card(page, "e2eupa");
      await expect(c).toContainText("Update to 0.2.0 is ready.");
      await c.getByRole("button", { name: "Update to 0.2.0" }).click();
      const dialog = page.getByRole("dialog", { name: "Update e2eupa" });
      // The plan: notes, the data changes with the steps that resolve them, and the dependents.
      await expect(dialog.getByText("Plates are now called registrations.")).toBeVisible();
      await expect(dialog.getByText("What changes in the data")).toBeVisible();
      await expect(dialog.getByText("Data steps that run with the update")).toBeVisible();
      await expect(
        dialog
          .getByRole("listitem")
          .filter({ hasText: /^rename e2eupa_item.plate to registration$/ }),
      ).toBeVisible();
      await expect(
        dialog.getByRole("listitem").filter({ hasText: /^fill empty e2eupa_item.tier/ }),
      ).toBeVisible();
      await expect(dialog.getByText("No installed module depends on this one.")).toBeVisible();
      // Nothing happened yet.
      expect(await version(api, "e2eupa")).toBe("0.1.0");
      await dialog.getByRole("button", { name: "Upgrade" }).click();
      await expect(toast(page, "Updated e2eupa")).toBeVisible();

      expect(await version(api, "e2eupa")).toBe("0.2.0");
      const rows = await api.list("root", "e2eupa_item");
      const byTitle = Object.fromEntries(rows.map((r: any) => [r.title, r]));
      expect(byTitle.first.registration).toBe("AB-1");
      expect(byTitle.second.registration).toBe("CD-2");
      expect(byTitle.first.tier).toBe("standard");
      expect(byTitle.first.plate ?? null).toBeNull();

      // The update is in the audit log, with the data steps named.
      const events = await api.run("root", "security_event.list", { limit: 200 });
      expect(
        events.some(
          (e: any) =>
            e.kind === "module_update" &&
            e.detail.includes("rename e2eupa_item.plate to registration"),
        ),
      ).toBe(true);

      // The way back: the Versions tab offers it, and taking it reverses the rename.
      await page.goto(`/#${MODULES}&id=e2eupa&section=versions`);
      const panel = page.getByRole("complementary", { name: "e2eupa module" });
      await expect(panel.getByText("Version 0.1.0 is kept.")).toBeVisible();
      await panel.getByRole("button", { name: "Roll back to 0.1.0" }).click();
      const back = page.getByRole("alertdialog", { name: "Roll e2eupa back to 0.1.0?" });
      await expect(back).toContainText("renamed by the update are renamed back");
      await back.getByRole("button", { name: "Roll back" }).click();
      await expect(toast(page, "Rolled e2eupa back")).toBeVisible();
      expect(await version(api, "e2eupa")).toBe("0.1.0");
      const again = await api.list("root", "e2eupa_item");
      expect(again.find((r: any) => r.title === "first").plate).toBe("AB-1");
    } finally {
      await clean(api, "e2eupa");
    }
  });

  test("an update that would break a record is refused and says why; an explicit drop step lets it through but cannot be rolled back", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await install(api, pkgText({ id: "e2eupb", version: "0.1.0", fields: field("legacy") }));
      await api.create("root", "e2eupb_item", { title: "x", legacy: "keep me?" });
      offer({ id: "e2eupb", version: "0.2.0", changelog: "Drops the legacy field." });

      await signIn("root", UPDATES);
      const c = card(page, "e2eupb");
      await expect(c).toContainText("Update to 0.2.0 is blocked");
      await c.getByRole("button", { name: "Update to 0.2.0" }).click();
      const dialog = page.getByRole("dialog", { name: "Update e2eupb" });
      await expect(dialog.getByText("Existing records")).toBeVisible();
      await expect(dialog).toContainText("field legacy is removed");
      await expect(dialog).toContainText("drop_field");
      await expect(dialog.getByRole("button", { name: "Upgrade" })).toBeDisabled();
      await dialog.getByRole("button", { name: "Cancel" }).click();
      expect(await version(api, "e2eupb")).toBe("0.1.0");

      // The author declares the step, with the confirmation that values are deleted.
      offer({
        id: "e2eupb",
        version: "0.2.0",
        changelog: "Drops the legacy field.",
        migrations: `
[[migration]]
from = "<0.2.0"
op = "drop_field"
model = "e2eupb_item"
field = "legacy"
confirm = true
`,
      });
      await page.reload();
      await card(page, "e2eupb").getByRole("button", { name: "Update to 0.2.0" }).click();
      const ok = page.getByRole("dialog", { name: "Update e2eupb" });
      await expect(ok).toContainText("deletes the values of e2eupb_item.legacy");
      await ok.getByRole("button", { name: "Upgrade" }).click();
      await expect(toast(page, "Updated e2eupb")).toBeVisible();
      expect(await version(api, "e2eupb")).toBe("0.2.0");

      await page.goto(`/#${MODULES}&id=e2eupb&section=versions`);
      const panel = page.getByRole("complementary", { name: "e2eupb module" });
      await expect(panel.getByText("It cannot be rolled back from here")).toBeVisible();
      await expect(panel).toContainText("restore the backup");
      await expect(panel.getByRole("button", { name: /Roll back/ })).toHaveCount(0);
    } finally {
      await clean(api, "e2eupb");
    }
  });

  test("an update that leaves a dependent's range is blocked, the plan names the dependent and its range, and ranges show in the panel", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await install(api, pkgText({ id: "e2eupc", version: "1.0.0" }));
      await install(
        api,
        pkgText({ id: "e2eupd", version: "0.1.0", deps: { e2eupc: ">=1.0, <2" } }),
      );
      offer({ id: "e2eupc", version: "2.0.0", changelog: "A breaking release." });

      await signIn("root", UPDATES);
      const c = card(page, "e2eupc");
      await expect(c).toContainText("Update to 2.0.0 is blocked");
      await c.getByRole("button", { name: "Update to 2.0.0" }).click();
      const dialog = page.getByRole("dialog", { name: "Update e2eupc" });
      const table = dialog.getByRole("table", {
        name: "Installed modules that depend on this one, and the versions they support",
      });
      await expect(table).toContainText("e2eupd");
      await expect(table).toContainText("≥1.0, <2");
      await expect(table).toContainText("Would break");
      await expect(dialog).toContainText("would break e2eupd");
      await expect(dialog.getByRole("button", { name: "Upgrade" })).toBeDisabled();
      await dialog.getByRole("button", { name: "Cancel" }).click();
      expect(await version(api, "e2eupc")).toBe("1.0.0");

      // The dependent's panel lists the versions it supports, core and modules alike.
      await page.goto(`/#${MODULES}&id=e2eupd&section=dependencies`);
      const panel = page.getByRole("complementary", { name: "e2eupd module" });
      const ranges = panel.getByRole("table", {
        name: "The versions of the core and of other modules e2eupd supports",
      });
      await expect(ranges).toContainText("Core");
      await expect(ranges).toContainText("≥1.0, <2");
      await expect(ranges).toContainText("1.0.0");
      await expect(ranges.getByText("Supported").first()).toBeVisible();
    } finally {
      await clean(api, "e2eupd", "e2eupc");
    }
  });

  test("update all takes the modules that fit and leaves the blocked one with its reason", async ({
    page,
    signIn,
    api,
  }) => {
    try {
      await install(api, pkgText({ id: "e2eupe", version: "0.1.0" }));
      await install(api, pkgText({ id: "e2eupf", version: "0.1.0" }));
      await install(api, pkgText({ id: "e2eupg", version: "1.0.0" }));
      await install(
        api,
        pkgText({ id: "e2euph", version: "0.1.0", deps: { e2eupg: ">=1.0, <2" } }),
      );
      offer({ id: "e2eupe", version: "0.2.0" });
      offer({ id: "e2eupf", version: "0.1.1" });
      offer({ id: "e2eupg", version: "2.0.0" });

      await signIn("root", UPDATES);
      await expect(page.getByText("2 of 3 can be updated now.")).toBeVisible();
      await page.getByRole("button", { name: "Update all compatible" }).click();
      const dialog = page.getByRole("dialog", { name: "Update all compatible modules" });
      await expect(dialog.getByRole("list", { name: "Will be updated" })).toContainText("e2eupe");
      await expect(dialog.getByRole("list", { name: "Will be updated" })).toContainText("e2eupf");
      await expect(dialog.getByText("Will stay as they are")).toBeVisible();
      await expect(dialog).toContainText("would break e2euph");
      await dialog.getByRole("button", { name: "Update 2 modules" }).click();
      // One was left alone, so the dialog shows the result before it closes.
      await expect(dialog.getByText("Left as they were")).toBeVisible();
      await expect(dialog).toContainText("would break e2euph");
      await dialog.getByRole("button", { name: "Close", exact: true }).last().click();
      expect(await version(api, "e2eupe")).toBe("0.2.0");
      expect(await version(api, "e2eupf")).toBe("0.1.1");
      expect(await version(api, "e2eupg")).toBe("1.0.0");
      await expect(card(page, "e2eupg")).toContainText("blocked");
    } finally {
      await clean(api, "e2euph", "e2eupe", "e2eupf", "e2eupg");
    }
  });

  test("every module must declare its core range and a range for each dependency", async ({
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const token = await api.token("root");
    const bare = pkgText({ id: "e2eupi", version: "0.1.0" }).replace(/requires_core = .*\n/, "");
    const r = await api.post("/api/module/install", { manifest: bare }, token);
    expect(r.status).toBe(400);
    expect(r.json.error).toContain("requires_core is required");
    const any = pkgText({ id: "e2eupi", version: "0.1.0" }).replace(
      "[[model]]",
      '[module.dependencies]\nbase = ""\n[[model]]',
    );
    // A dependency with an empty range is refused with the dependency named.
    const e = await api.post("/api/module/install", { manifest: any }, token);
    expect(e.status).toBe(400);
    expect(e.json.error).toContain("base");
    expect(e.json.error).toContain("range is empty");
    // The old any-version shorthand is gone.
    const short = pkgText({ id: "e2eupi", version: "0.1.0" }).replace(
      /requires_core = .*\n/,
      (m) => `${m}depends = ["base"]\n`,
    );
    const s = await api.post("/api/module/install", { manifest: short }, token);
    expect(s.status).toBe(400);
    expect(s.json.error).toContain("depends = [...] is no longer accepted");
    expect(await version(api, "e2eupi")).toBeUndefined();
  });

  test("an agent can only propose an update; a person approves it", async ({ api }) => {
    try {
      await install(api, pkgText({ id: "e2eupj", version: "0.1.0" }));
      offer({ id: "e2eupj", version: "0.2.0" });
      const root = await api.token("root");
      const agent = await api.post(
        "/api/auth/agent-token",
        { scopes: ["platform.admin"], label: "updater" },
        root,
      );
      expect(agent.status).toBe(200);
      const proposed = await api.post("/api/module/upgrade", { id: "e2eupj" }, agent.json.token);
      expect(JSON.stringify(proposed.json)).toMatch(/plan_id|approval/);
      expect(await version(api, "e2eupj")).toBe("0.1.0");
    } finally {
      await clean(api, "e2eupj");
    }
  });
});
