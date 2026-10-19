import { users } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// Accounts and sessions (B7): your own password and sessions, and the administrator's list of
// people with disable, enable and a password reset. Each test owns its users (jo, pat).
const SECURITY = "/customize/security?tab=security";

test.describe("accounts and sessions", () => {
  test("you see your sessions, end the others, change your password and sign out everywhere", async ({
    page,
    api,
    signIn,
    allowHttpErrors,
  }) => {
    test.setTimeout(60_000);
    allowHttpErrors(401);
    const { name, password } = users.jo;
    // A second session, as from another device.
    const other = await api.post("/api/auth/login", { username: name, password });
    expect(other.status).toBe(200);
    const otherToken = other.json.token as string;
    await signIn("jo", SECURITY);

    // The list: two sessions, one of them this one. Nothing in it is a credential.
    const sessions = page.getByRole("table", { name: "Your sessions" });
    await expect(sessions).toBeVisible();
    await expect(sessions.locator("tbody tr")).toHaveCount(2);
    await expect(sessions.getByText("This session")).toHaveCount(1);
    expect(await sessions.textContent()).not.toContain(otherToken);

    // End the other one.
    await page.getByRole("button", { name: "Sign out other sessions" }).click();
    await expect(page.getByText("1 other session signed out.")).toBeVisible();
    await expect(sessions.locator("tbody tr")).toHaveCount(1);
    expect((await api.post("/api/party/list", {}, otherToken)).status).toBe(401);
    await expect(page.getByRole("button", { name: "Sign out other sessions" })).toBeDisabled();

    // The password: the form refuses what cannot work before it asks the server.
    const next = "e2e jo new password 2!";
    const change = page.getByRole("button", { name: "Change password" });
    await page.getByLabel("Current password").fill(password);
    await page.getByLabel(/^New password/).fill("short");
    await expect(page.getByText("At least 12 characters.").last()).toBeVisible();
    await expect(change).toBeDisabled();
    await page.getByLabel(/^New password/).fill(next);
    await page.getByLabel("Repeat the new password").fill(`${next} typo`);
    await expect(page.getByText("The new passwords do not match.")).toBeVisible();
    await expect(change).toBeDisabled();
    // A wrong current password is said plainly and changes nothing.
    await page.getByLabel("Repeat the new password").fill(next);
    await page.getByLabel("Current password").fill("not the password");
    await change.click();
    await expect(page.getByText("The current password is not right.")).toBeVisible();
    // The right one changes it, signs out nothing of this session, and the old one stops working.
    await page.getByLabel("Current password").fill(password);
    await change.click();
    await expect(
      page.getByText("Password changed. Your other sessions were signed out."),
    ).toBeVisible();
    expect((await api.post("/api/auth/login", { username: name, password })).status).toBe(401);
    const fresh = await api.post("/api/auth/login", { username: name, password: next });
    expect(fresh.status).toBe(200);
    await expect(page.locator(".topbar")).toBeVisible();

    // Sign out everywhere ends this session too: the sign-in form comes back.
    await page.getByRole("button", { name: "Sign out everywhere" }).click();
    await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);
    expect((await api.post("/api/party/list", {}, fresh.json.token)).status).toBe(401);
  });

  test("an administrator disables, enables and resets an account, and it is all in the log", async ({
    page,
    api,
    signIn,
    allowHttpErrors,
  }) => {
    test.setTimeout(90_000);
    allowHttpErrors(400, 401, 403);
    const { name, password } = users.pat;
    // pat has a session that the administrator's actions must end.
    const patToken = (await api.post("/api/auth/login", { username: name, password })).json
      .token as string;
    expect((await api.post("/api/party/list", {}, patToken)).status).toBe(200);
    await signIn("root", SECURITY);

    const people = page.getByRole("table", { name: "People" });
    await expect(people).toBeVisible();
    const row = people
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name, exact: true }) });
    await expect(row).toContainText("Active");

    // Disable: the person is signed out at once and cannot sign in.
    await row.getByRole("button", { name: "Disable" }).click();
    await expect(page.getByText(`${name} is disabled and signed out.`)).toBeVisible();
    await expect(row).toContainText("Disabled");
    expect((await api.post("/api/party/list", {}, patToken)).status).toBe(401);
    expect((await api.post("/api/auth/login", { username: name, password })).status).toBe(401);
    await row.getByRole("button", { name: "Enable" }).click();
    await expect(page.getByText(`${name} can sign in again.`)).toBeVisible();
    await expect(row).toContainText("Active");
    expect((await api.post("/api/auth/login", { username: name, password })).status).toBe(200);

    // Nobody can lock themselves out of the people list.
    const own = people
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: "root", exact: true }) });
    await own.getByRole("button", { name: "Disable" }).click();
    await expect(page.getByText("you cannot disable your own account")).toBeVisible();

    // Reset: a temporary password; the person must choose their own before anything else.
    const temp = "e2e temporary pw 3!";
    await row.getByRole("button", { name: "Reset password" }).click();
    await page.getByLabel(`Temporary password for ${name}`).fill(temp);
    await page.getByRole("button", { name: "Set temporary password" }).click();
    await expect(page.getByText(/Temporary password set for pat\./)).toBeVisible();
    await expect(row).toContainText("Must choose a password");
    expect((await api.post("/api/auth/login", { username: name, password })).status).toBe(401);

    // The person signs in with it (in the app) and meets only the new-password screen.
    const login = await api.post("/api/auth/login", { username: name, password: temp });
    expect(login.json.password_change_required).toBe(true);
    const limited = login.json.token as string;
    const refused = await api.get("/api/meta", limited);
    expect([refused.status, refused.json.code]).toEqual([403, "password_change_required"]);
    const second = await page.context().newPage();
    await second.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [limited, name],
    );
    await second.goto("/");
    await expect(second.getByText("Choose a new password to continue")).toBeVisible();
    await expect(second.locator(".topbar")).toHaveCount(0);
    const chosen = "e2e pat chosen pw 4!";
    await second.getByLabel("Current password").fill(temp);
    await second.getByLabel(/^New password/).fill(chosen);
    await second.getByLabel("Repeat the new password").fill(chosen);
    await second.getByRole("button", { name: "Change password" }).click();
    await expect(second.locator(".topbar")).toBeVisible();
    expect((await api.post("/api/auth/login", { username: name, password: chosen })).status).toBe(
      200,
    );
    await second.close();

    // A change to the sign-in policy is written to the log too (and put back).
    const before = (await api.run("root", "setting.get", { id: "instance" })).value as string;
    try {
      await api.run("root", "setting.update", {
        id: "instance",
        value: JSON.stringify({ ...JSON.parse(before), security: { require_mfa_for_all: true } }),
      });
    } finally {
      await api.run("root", "setting.update", { id: "instance", value: before });
    }

    // The log shows who did what, and no password anywhere.
    await page.reload();
    const log = page.getByRole("table", { name: "Security log" });
    await expect(log).toBeVisible();
    for (const what of [
      "Account disabled",
      "Account enabled",
      "Password reset by an administrator",
      "Security setting changed",
      "Password changed",
    ])
      await expect(log.getByRole("row").filter({ hasText: what }).first()).toBeVisible();
    const text = (await log.textContent()) ?? "";
    for (const secret of [password, temp, chosen]) expect(text).not.toContain(secret);
    await expect(
      log.getByRole("row").filter({ hasText: "Account disabled" }).first(),
    ).toContainText("root");
  });

  test("people who are not administrators get no list of accounts", async ({
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(404);
    const sam = await api.token("sam");
    expect((await api.get("/api/auth/users", sam)).status).toBe(404);
    expect((await api.post("/api/auth/users/jo/disable", {}, sam)).status).toBe(404);
    expect(
      (await api.post("/api/auth/users/jo/reset-password", { password: "x".repeat(14) }, sam))
        .status,
    ).toBe(404);
    const root = await api.token("root");
    const list = await api.get("/api/auth/users", root);
    expect(list.status).toBe(200);
    expect(JSON.stringify(list.json)).not.toContain("argon2");
  });
});
