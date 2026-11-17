import { expect, test } from "../support/fixtures.ts";

// D69: an administrator creates a person, gives them access by app (User or Manager of Sales, CRM)
// and offboards them, all from Customize > Security > People, and the security log says who did
// what. Access by app never shows a group identifier unless "Show technical names" is on.
const SECURITY = "/customize/security?tab=security";

test.describe("people administration", () => {
  test("create a person, change their access, offboard them, enable them again", async ({
    page,
    api,
    signIn,
    allowHttpErrors,
  }) => {
    test.setTimeout(120_000);
    allowHttpErrors(400, 401, 403, 404);
    const name = "e2e-nia";
    // An app with a User and a Manager rung, installed for this test and removed after it.
    const had = (await api.run("root", "module.list")).some(
      (m: { id: string }) => m.id === "inventory",
    );
    if (!had) await api.run("root", "marketplace_listing.install", { id: "inventory" });
    try {
      await signIn("root", SECURITY);
      const people = page.getByRole("table", { name: "People" });
      await expect(people).toBeVisible();
      const row = people
        .getByRole("row")
        .filter({ has: page.getByRole("cell", { name, exact: true }) });

      // Create: Sales as User, leave the password empty, get a temporary one once.
      await page.getByRole("button", { name: "New person" }).click();
      const dialog = page.getByRole("dialog", { name: "New person" });
      await dialog.getByLabel("Username").fill(name);
      // Sales has one group: one switch.
      await dialog.getByRole("switch", { name: "Can use Sales" }).check();
      await dialog.getByRole("button", { name: "Create person" }).click();
      const shown = page.getByRole("dialog", { name: "Temporary password" });
      await expect(shown).toBeVisible();
      const temporary = (await shown.getByTestId("temp-password").textContent())!.trim();
      expect(temporary.length).toBeGreaterThanOrEqual(12);
      await shown.getByRole("button", { name: "Done" }).click();
      await expect(row).toContainText("Must choose a password");
      await expect(row).toContainText("Sales · User");

      // The temporary password works once, to choose a real one.
      const login = await api.post("/api/auth/login", { username: name, password: temporary });
      expect(login.json.password_change_required).toBe(true);
      const chosen = "e2e nia chosen pw 5!";
      const changed = await api.post(
        "/api/auth/password",
        { current_password: temporary, new_password: chosen },
        login.json.token as string,
      );
      expect(changed.status).toBe(200);
      const nia = (await api.post("/api/auth/login", { username: name, password: chosen })).json
        .token as string;
      expect((await api.post("/api/sale_order/list", {}, nia)).status).toBe(200);
      expect((await api.post("/api/lead/list", {}, nia)).status).toBe(404);

      // Access: give CRM and raise Inventory (an app with User and Manager) to Manager in the
      // dialog; her open session sees it at once. Manager holds both groups of Inventory.
      await row.getByRole("button", { name: `Access of ${name}` }).click();
      const access = page.getByRole("dialog", { name: `Access of ${name}` });
      await expect(access.getByRole("switch", { name: "Can use Sales" })).toBeChecked();
      await access.getByRole("switch", { name: "Can use CRM" }).check();
      await access
        .getByRole("radiogroup", { name: "Access to Inventory" })
        .getByRole("radio", { name: "Manager", exact: true })
        .click();
      await access.getByRole("button", { name: "Save access" }).click();
      await expect(page.getByText(`Access of ${name} saved.`)).toBeVisible();
      await expect(row).toContainText("Inventory · Manager");
      await expect(row).toContainText("CRM · User");
      await expect(row).toContainText("Sales · User");
      expect((await api.post("/api/lead/list", {}, nia)).status).toBe(200);
      const stored = await api.get("/api/auth/users", await api.token("root"));
      const held = (stored.json.users as { name: string; groups: string[] }[]).find(
        (u) => u.name === name,
      )!.groups;
      expect(held.sort()).toEqual([
        "crm.user",
        "inventory.manager",
        "inventory.user",
        "sales.user",
      ]);
      // The identifiers show only on request.
      await row.getByRole("button", { name: `Access of ${name}` }).click();
      const again = page.getByRole("dialog", { name: `Access of ${name}` });
      await expect(again.locator("code")).toHaveCount(0);
      await again.getByRole("switch", { name: "Show technical names" }).check();
      await expect(again.locator("code", { hasText: "inventory.manager" })).toBeVisible();
      await again.getByRole("switch", { name: "Show technical names" }).uncheck();
      await again.getByRole("button", { name: "Cancel" }).click();
      await expect(row.getByRole("button", { name: `Access of ${name}` })).toBeFocused();

      // Offboard: confirm in the dialog; every credential stops for good.
      const agent = (await api.post("/api/auth/agent-token", { scopes: ["sales.user"] }, nia)).json
        .token as string;
      expect((await api.post("/api/sale_order/list", {}, agent)).status).toBe(200);
      await row.getByRole("button", { name: `More actions for ${name}` }).click();
      await page.getByRole("menuitem", { name: "Offboard" }).click();
      const confirm = page.getByRole("alertdialog", { name: `Offboard ${name}?` });
      await confirm.getByLabel("Note (optional)").fill("e2e leaver");
      await confirm.getByRole("button", { name: "Offboard" }).click();
      await expect(
        page.getByText(/is offboarded: 2 session\(s\), 1 agent token\(s\)/),
      ).toBeVisible();
      await expect(row).toContainText("Offboarded");
      for (const t of [nia, agent])
        expect((await api.post("/api/sale_order/list", {}, t)).status).toBe(401);
      expect((await api.post("/api/auth/login", { username: name, password: chosen })).status).toBe(
        401,
      );

      // Enable again: she can sign in, but nothing revoked comes back.
      await row.getByRole("button", { name: `More actions for ${name}` }).click();
      await page.getByRole("menuitem", { name: "Enable" }).click();
      await expect(page.getByText(`${name} can sign in again.`)).toBeVisible();
      await expect(row).toContainText("Active");
      expect((await api.post("/api/auth/login", { username: name, password: chosen })).status).toBe(
        200,
      );
      for (const t of [nia, agent])
        expect((await api.post("/api/sale_order/list", {}, t)).status).toBe(401);

      // The log names who did it and what changed.
      await page.reload();
      const log = page.getByRole("table", { name: "Security log" });
      await expect(log).toContainText("Account offboarded");
      await expect(log).toContainText("Groups changed");
      await expect(log).toContainText("Account created");
      await expect(log).toContainText("e2e leaver");
    } finally {
      if (!had) await api.run("root", "module.uninstall", { id: "inventory" });
    }
  });

  test("nobody can take the last administrator away or lock themselves out", async ({
    page,
    signIn,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await signIn("root", SECURITY);
    const people = page.getByRole("table", { name: "People" });
    const own = people
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: "root", exact: true }) });
    await own.getByRole("button", { name: "Access of root" }).click();
    const access = page.getByRole("dialog", { name: "Access of root" });
    // Your own administrator access cannot be turned off here, and the dialog says why.
    const admin = access.getByRole("switch", { name: "Administrator: can do everything" });
    await expect(admin).toBeChecked();
    await expect(admin).toBeDisabled();
    await expect(
      access.getByText(
        "You cannot take administrator access from yourself; ask another administrator.",
      ),
    ).toBeVisible();
    await access.getByRole("button", { name: "Cancel" }).click();
    await own.getByRole("button", { name: "More actions for root" }).click();
    await page.getByRole("menuitem", { name: "Offboard" }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "Offboard" }).click();
    await expect(page.getByText("you cannot offboard your own account")).toBeVisible();
  });
});
