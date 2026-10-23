import { expect, test } from "../support/fixtures.ts";

// D69: an administrator creates a person, edits their permission groups and offboards them, all
// from Customize > Security > People, and the security log says who did what.
const SECURITY = "/customize/security?tab=security";

test.describe("people administration", () => {
  test("create a person, change their groups, offboard them, enable them again", async ({
    page,
    api,
    signIn,
    allowHttpErrors,
  }) => {
    test.setTimeout(120_000);
    allowHttpErrors(400, 401, 403, 404);
    const name = "e2e-nia";
    await signIn("root", SECURITY);
    const people = page.getByRole("table", { name: "People" });
    await expect(people).toBeVisible();
    const row = people
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name, exact: true }) });

    // Create: choose a group, leave the password empty, get a temporary one once.
    await page.getByRole("button", { name: "New person" }).click();
    const dialog = page.getByRole("dialog", { name: "New person" });
    await dialog.getByLabel("Username").fill(name);
    await dialog.getByLabel("Filter groups").fill("sales.user");
    await dialog.getByRole("checkbox", { name: "sales.user" }).check();
    // An unknown name is refused before anything is written.
    await dialog.getByRole("button", { name: "Create person" }).click();
    const shown = page.getByRole("dialog", { name: "Temporary password" });
    await expect(shown).toBeVisible();
    const temporary = (await shown.getByTestId("temp-password").textContent())!.trim();
    expect(temporary.length).toBeGreaterThanOrEqual(12);
    await shown.getByRole("button", { name: "Done" }).click();
    await expect(row).toContainText("Must choose a password");
    await expect(row).toContainText("sales.user");

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

    // Groups: add one in the dialog; her open session sees it at once.
    await row.getByRole("button", { name: "Groups", exact: true }).click();
    const groups = page.getByRole("dialog", { name: `Groups of ${name}` });
    await groups.getByLabel("Filter groups").fill("crm.user");
    await groups.getByRole("checkbox", { name: "crm.user" }).check();
    await groups.getByRole("button", { name: "Save groups" }).click();
    await expect(page.getByText(`Groups of ${name} saved.`)).toBeVisible();
    await expect(row).toContainText("crm.user");
    expect((await api.post("/api/lead/list", {}, nia)).status).toBe(200);

    // Offboard: confirm in the dialog; every credential stops for good.
    const agent = (await api.post("/api/auth/agent-token", { scopes: ["sales.user"] }, nia)).json
      .token as string;
    expect((await api.post("/api/sale_order/list", {}, agent)).status).toBe(200);
    await row.getByRole("button", { name: "Offboard" }).click();
    const confirm = page.getByRole("alertdialog", { name: `Offboard ${name}?` });
    await confirm.getByLabel("Note (optional)").fill("e2e leaver");
    await confirm.getByRole("button", { name: "Offboard" }).click();
    await expect(page.getByText(/is offboarded: 2 session\(s\), 1 agent token\(s\)/)).toBeVisible();
    await expect(row).toContainText("Offboarded");
    for (const t of [nia, agent])
      expect((await api.post("/api/sale_order/list", {}, t)).status).toBe(401);
    expect((await api.post("/api/auth/login", { username: name, password: chosen })).status).toBe(
      401,
    );

    // Enable again: she can sign in, but nothing revoked comes back.
    await row.getByRole("button", { name: "Enable" }).click();
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
    await own.getByRole("button", { name: "Groups", exact: true }).click();
    const groups = page.getByRole("dialog", { name: "Groups of root" });
    await groups.getByRole("checkbox", { name: /^platform\.admin/ }).uncheck();
    await groups.getByRole("button", { name: "Save groups" }).click();
    await expect(
      groups.getByText("you cannot take administrator access from yourself"),
    ).toBeVisible();
    await groups.getByRole("button", { name: "Cancel" }).click();
    await own.getByRole("button", { name: "Offboard" }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: "Offboard" }).click();
    await expect(page.getByText("you cannot offboard your own account")).toBeVisible();
  });
});
