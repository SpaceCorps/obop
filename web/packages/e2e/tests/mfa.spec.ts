import { readFile } from "node:fs/promises";
import type { Page } from "@playwright/test";
import { platformUrl, users, type UserName } from "../support/env.ts";
import { expect, test, type Api } from "../support/fixtures.ts";
import { base32Decode, hotp, STEP } from "../support/totp.ts";

const SECURITY = "/#/customize/security?tab=security";
const stepNow = () => Math.floor(Date.now() / 1000 / STEP);
const code = (secret: string, step: number) => hotp(base32Decode(secret), step);

type Enrolled = { secret: string; step: number; codes: string[] };

/** Run the setup panel to the end: key, code, recovery codes, "I saved them". */
async function enrol(page: Page): Promise<Enrolled> {
  await page.getByRole("button", { name: "Set up two-factor" }).click();
  const qr = page.getByRole("img", { name: "QR code for the authenticator app" });
  await expect(qr).toBeVisible();
  // A real symbol: one path with many modules, drawn here, never fetched.
  expect(((await qr.locator("path").getAttribute("d")) ?? "").length).toBeGreaterThan(200);
  const secret = ((await page.getByLabel("Setup key").textContent()) ?? "").replace(/\s/g, "");
  expect(secret).toMatch(/^[A-Z2-7]{32}$/);
  const step = stepNow();
  await page.getByLabel("6-digit code").fill(code(secret, step));
  await page.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByRole("heading", { name: "Save your recovery codes" })).toBeVisible();
  const codes = await page
    .getByRole("list", { name: "Recovery codes" })
    .locator("code")
    .allTextContents();
  expect(codes).toHaveLength(10);
  expect(new Set(codes).size).toBe(10);
  return { secret, step, codes };
}

async function saved(page: Page) {
  const done = page.getByRole("button", { name: "Done" });
  await expect(done).toBeDisabled();
  await page.getByLabel("I have saved these codes").check();
  await done.click();
}

async function signInForm(page: Page, user: UserName) {
  await page.goto("/");
  await page.getByLabel("Username").fill(users[user].name);
  await page.getByLabel("Password").fill(users[user].password);
  await page.getByRole("button", { name: "Sign in" }).click();
}

/** Enrol a user through the API (arranging data, not under test). */
async function enrolViaApi(api: Api, user: UserName): Promise<Enrolled> {
  const token = await api.token(user);
  const begun = await api.post("/api/auth/mfa/begin", {}, token);
  expect(begun.status).toBe(200);
  const step = stepNow();
  const done = await api.post(
    "/api/auth/mfa/confirm",
    { code: code(begun.json.secret, step) },
    token,
  );
  expect(done.status, JSON.stringify(done.json)).toBe(200);
  return { secret: begun.json.secret, step, codes: done.json.recovery_codes };
}

test.describe("two-factor authentication", () => {
  test("enrol, sign in with a code and a recovery code, then turn it off", async ({
    page,
    api,
    allowHttpErrors,
  }) => {
    test.setTimeout(60_000);
    allowHttpErrors(401);
    // Signed in through the form: the signIn fixture would store the token again on every load.
    await signInForm(page, "tess");
    await expect(page.locator(".topbar")).toBeVisible();
    await page.goto(SECURITY);
    await expect(page.getByRole("heading", { name: /^Two-factor authentication/ })).toBeVisible();
    await expect(page.getByText("Off", { exact: true })).toBeVisible();

    // Nothing is active before the first correct code: a wrong one changes nothing.
    await page.getByRole("button", { name: "Set up two-factor" }).click();
    await page.getByLabel("6-digit code").fill("000000");
    await page.getByRole("button", { name: "Turn on" }).click();
    await expect(
      page.getByText("That code is not valid. Check the time on your device and try again."),
    ).toBeVisible();
    await page.getByRole("button", { name: "Cancel" }).click();

    const { secret, step, codes } = await enrol(page);
    // Recovery codes can be saved as a file.
    const [file] = await Promise.all([
      page.waitForEvent("download"),
      page.getByRole("button", { name: "Download" }).click(),
    ]);
    expect(file.suggestedFilename()).toBe("obop-recovery-codes.txt");
    const text = await readFile((await file.path())!, "utf8");
    for (const c of codes) expect(text).toContain(c);
    await saved(page);
    await expect(page.getByText("10 recovery codes left.")).toBeVisible();
    await expect(page.getByText("On", { exact: true })).toBeVisible();

    // The password alone no longer opens a session.
    const first = await api.post("/api/auth/login", {
      username: users.tess.name,
      password: users.tess.password,
    });
    expect(first.json.mfa_required).toBe(true);
    expect(first.json.token).toBeUndefined();

    // Sign in: the second step has the right input hints and focus, and a wrong code is refused.
    await page.evaluate(() => sessionStorage.clear());
    await signInForm(page, "tess");
    await expect(page.getByRole("heading", { name: "Two-factor authentication" })).toBeVisible();
    const input = page.getByLabel("6-digit code");
    await expect(input).toBeFocused();
    await expect(input).toHaveAttribute("autocomplete", "one-time-code");
    await expect(input).toHaveAttribute("inputmode", "numeric");
    await input.fill("000000");
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.getByText(/That code is not valid\./)).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);
    // The next step's code is accepted (one step of clock drift either way), and only once.
    await input.fill(code(secret, step + 1));
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.locator(".topbar")).toBeVisible();

    // A recovery code signs in once.
    await page.evaluate(() => sessionStorage.clear());
    await signInForm(page, "tess");
    await page.getByRole("button", { name: "Use a recovery code" }).click();
    await page.getByLabel("Recovery code").fill(codes[0]!);
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    await page.evaluate(() => sessionStorage.clear());
    await signInForm(page, "tess");
    await page.getByRole("button", { name: "Use a recovery code" }).click();
    await page.getByLabel("Recovery code").fill(codes[0]!);
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.getByText(/That code is not valid\./)).toBeVisible();
    // Back to the password step works.
    await page.getByRole("button", { name: "Back to sign in" }).click();
    await expect(page.getByLabel("Password")).toBeVisible();

    // Turn it off with the password and a recovery code.
    await page.getByLabel("Username").fill(users.tess.name);
    await page.getByLabel("Password").fill(users.tess.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("button", { name: "Use a recovery code" }).click();
    await page.getByLabel("Recovery code").fill(codes[1]!);
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    await page.goto(SECURITY);
    await expect(page.getByText("8 recovery codes left.")).toBeVisible();
    await page.getByRole("button", { name: "Turn off two-factor" }).click();
    await page.getByLabel("Password", { exact: true }).fill("not the right password");
    await page.getByRole("button", { name: "Use a recovery code" }).click();
    await page.getByLabel("Recovery code").fill(codes[2]!);
    await page.getByRole("button", { name: "Turn off", exact: true }).click();
    await expect(page.getByText("That password is not right.")).toBeVisible();
    await page.getByLabel("Password", { exact: true }).fill(users.tess.password);
    await page.getByRole("button", { name: "Turn off", exact: true }).click();
    await expect(page.getByText("Two-factor authentication is off")).toBeVisible();
    await expect(page.getByRole("button", { name: "Set up two-factor" })).toBeVisible();
    const plain = await api.post("/api/auth/login", {
      username: users.tess.name,
      password: users.tess.password,
    });
    expect(plain.status).toBe(200);
    expect(plain.json.token).toBeTruthy();
  });

  test("wrong codes lock the account out with a clear message", async ({
    page,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(401, 429);
    await enrolViaApi(api, "lock");
    await signInForm(page, "lock");
    const input = page.getByLabel("6-digit code");
    for (let i = 0; i < 5; i++) {
      await input.fill("000000");
      await page.getByRole("button", { name: "Verify" }).click();
      await expect(page.getByText(/That code is not valid\./)).toBeVisible();
    }
    await input.fill("000000");
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.getByText("Too many attempts. Wait a minute and try again.")).toBeVisible();
    // The password step is locked as well, and says so.
    await page.getByRole("button", { name: "Back to sign in" }).click();
    await page.getByLabel("Username").fill(users.lock.name);
    await page.getByLabel("Password").fill(users.lock.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("alert")).toHaveText(
      "Too many attempts. Wait a minute and try again.",
    );
    await expect(page.locator(".topbar")).toHaveCount(0);
  });

  test("a new admin must set it up before anything else works", async ({
    page,
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(403);
    const rootToken = await api.token("root");
    const before = (await api.run("root", "setting.get", { id: "instance" })).value as string;
    const withPolicy = (on: boolean) =>
      JSON.stringify({ ...JSON.parse(before), security: { require_mfa_for_admins: on } });
    await api.run("root", "setting.update", { id: "instance", value: withPolicy(true) });
    try {
      // The API says what is wrong and what to do.
      const login = await api.post("/api/auth/login", {
        username: users.nova.name,
        password: users.nova.password,
      });
      expect(login.json.mfa_enrolment_required).toBe(true);
      const limited = login.json.token as string;
      for (const path of ["/api/meta", "/api/commands"]) {
        const r = await api.get(path, limited);
        expect(r.status, path).toBe(403);
        expect(r.json.code).toBe("mfa_enrolment_required");
      }
      const blocked = await api.post("/api/party/list", {}, limited);
      expect([blocked.status, blocked.json.code]).toEqual([403, "mfa_enrolment_required"]);
      expect((await api.get("/api/auth/mfa", limited)).json.limited).toBe(true);

      // In the app the only screen is the setup, with the reason.
      await signInForm(page, "nova");
      await expect(page.getByText("Set up two-factor authentication to continue")).toBeVisible();
      await expect(page.locator(".topbar")).toHaveCount(0);
      await enrol(page);
      await saved(page);
      // Carrying on needs nothing else: the same session is lifted.
      await expect(page.locator(".topbar")).toBeVisible();
      await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
    } finally {
      await api.post("/api/setting/update", { id: "instance", value: before }, rootToken);
    }
    // The policy is off again, and a person who has it still needs the code.
    const again = await api.post("/api/auth/login", {
      username: users.nova.name,
      password: users.nova.password,
    });
    expect(again.json.mfa_required).toBe(true);
  });

  test("an admin resets someone's two-factor and the log shows who did it", async ({
    page,
    api,
    signIn,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    await enrolViaApi(api, "quinn");
    await signIn("root", "/customize/security?tab=security");
    await expect(page.getByRole("heading", { name: "Sign-in policy" })).toBeVisible();
    const adminsSwitch = page.getByRole("switch", { name: "Require 2FA for admins" });
    // The suite runs with the admin policy off (OBOP_REQUIRE_MFA_FOR_ADMINS=0).
    await expect(adminsSwitch).not.toBeChecked();

    await page.getByLabel("Username").fill("quinn");
    await page.getByRole("button", { name: "Reset two-factor" }).click();
    await expect(page.getByText("Two-factor reset for quinn", { exact: true })).toBeVisible();
    const log = page.getByRole("table", { name: "Security log" });
    const row = log.locator("tr", { hasText: "Two-factor reset" }).first();
    await expect(row).toContainText("quinn");
    await expect(row).toContainText("root");

    const plain = await api.post("/api/auth/login", {
      username: users.quinn.name,
      password: users.quinn.password,
    });
    expect(plain.json.token).toBeTruthy();
    // Someone with nothing to reset gets a plain answer, not a stack trace.
    await page.getByLabel("Username").fill("quinn");
    await page.getByRole("button", { name: "Reset two-factor" }).click();
    await expect(
      page.getByText("There is no such person, or they have no two-factor to reset."),
    ).toBeVisible();
  });

  test("agent tokens cannot enrol, disable or reset", async ({ api, allowHttpErrors }) => {
    allowHttpErrors(404);
    const sam = await api.token("sam");
    const issued = await api.post("/api/auth/agent-token", { scopes: ["base.user"] }, sam);
    expect(issued.status).toBe(200);
    const agent = issued.json.token as string;
    expect((await api.get("/api/auth/mfa", agent)).status).toBe(404);
    for (const path of ["begin", "confirm", "disable", "recovery", "reset"])
      expect((await api.post(`/api/auth/mfa/${path}`, { user: "sam" }, agent)).status, path).toBe(
        404,
      );
    // The agent still works for everything else it was scoped to.
    expect((await api.post("/api/party/list", {}, agent)).status).toBe(200);
  });
});

test.describe("two-factor authentication on the control plane (opt-in)", () => {
  test("an owner turns it on for their own account and signs in with a code", async ({
    page,
    uid,
  }) => {
    const email = `${uid("mfa").replace(/ /g, "-")}@example.test`;
    const password = "e2e platform password";
    await page.goto(`${platformUrl()}/app/`);
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Create account" }).click();
    await expect(page.getByRole("heading", { name: "Your instances" })).toBeVisible();

    await page.getByRole("button", { name: "Security" }).click();
    const dialog = page.getByRole("dialog", { name: "Account security" });
    const { secret, step } = await enrol(page);
    await saved(page);
    await expect(dialog.getByText("10 recovery codes left.")).toBeVisible();
    await dialog.getByRole("button", { name: "Close" }).click();

    await page.getByRole("button", { name: "Sign out" }).click();
    await page.getByRole("button", { name: "I already have an account" }).click();
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByLabel("6-digit code")).toBeFocused();
    await page.getByLabel("6-digit code").fill(code(secret, step + 1));
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.getByRole("heading", { name: "Your instances" })).toBeVisible();
  });
});
