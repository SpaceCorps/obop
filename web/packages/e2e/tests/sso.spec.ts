// Sign in with Microsoft (D91) in a real browser: the button, the round trip through a stand-in for
// Microsoft Entra on loopback, and what the page and the server keep afterwards. The stand-in is not
// Entra (see support/idp.ts); nothing here talks to Microsoft. The Rust tests cover the checks of the
// id token and the server's side of the flow; these tests cover what only a browser shows.
import type { APIRequestContext } from "@playwright/test";
import { idpUrl, sso, ssoUrl } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { base32Decode, hotp, STEP } from "../support/totp.ts";

const stepNow = () => Math.floor(Date.now() / 1000 / STEP);
const code = (secret: string, step: number) => hotp(base32Decode(secret), step);
const FAILED = /The sign-in with Microsoft could not be completed\./;

type Json = Record<string, any>;

async function post(request: APIRequestContext, path: string, body: Json, token?: string) {
  const res = await request.post(`${ssoUrl()}${path}`, {
    data: body,
    headers: token ? { authorization: `Bearer ${token}` } : {},
  });
  return { status: res.status(), json: (await res.json().catch(() => ({}))) as Json };
}

/** Enrol an authenticator app for `name` through the API (arranging data, not under test). */
async function enrol(request: APIRequestContext, name: keyof typeof sso.users) {
  const u = sso.users[name];
  const first = await post(request, "/api/auth/login", { username: u.name, password: u.password });
  expect(first.status, `login as ${name}`).toBe(200);
  const token = first.json.token as string;
  const begun = await post(request, "/api/auth/mfa/begin", {}, token);
  expect(begun.status).toBe(200);
  const step = stepNow();
  const done = await post(
    request,
    "/api/auth/mfa/confirm",
    { code: code(begun.json.secret, step) },
    token,
  );
  expect(done.status, JSON.stringify(done.json)).toBe(200);
  return {
    secret: begun.json.secret as string,
    step,
    recovery: done.json.recovery_codes as string[],
  };
}

/** Sign in as an enrolled administrator with a recovery code: a whole session. */
async function adminSession(
  request: APIRequestContext,
  name: keyof typeof sso.users,
  recovery: string,
) {
  const u = sso.users[name];
  const first = await post(request, "/api/auth/login", { username: u.name, password: u.password });
  expect(first.json.mfa_required).toBe(true);
  const second = await post(request, "/api/auth/login/mfa", {
    mfa_token: first.json.mfa_token,
    recovery_code: recovery,
  });
  expect(second.status, JSON.stringify(second.json)).toBe(200);
  return second.json.token as string;
}

let admin = "";
let bob: { secret: string; step: number } | undefined;

test.describe.configure({ mode: "serial" });

test.describe("sign in with Microsoft", () => {
  test.beforeAll(async ({ playwright }) => {
    // Four accounts with an authenticator app, each several Argon2 hashes of a debug binary: more
    // than the 30 seconds of a test when the machine is busy.
    test.setTimeout(120_000);
    const request = await playwright.request.newContext();
    // Three administrators keep a password and an authenticator app: the break-glass accounts
    // that an SSO-only account needs (D91).
    const root1 = await enrol(request, "root1");
    await enrol(request, "root2");
    await enrol(request, "root3");
    admin = await adminSession(request, "root1", root1.recovery[0]!);
    bob = await enrol(request, "bob");
    await request.dispose();
  });

  /** Click through the stand-in's page as `who` (or cancel). */
  async function atMicrosoft(page: import("@playwright/test").Page, who: string) {
    await expect(page).toHaveURL(new RegExp(`^${idpUrl()}/`));
    await page
      .getByRole("link", { name: who === "cancel" ? "Cancel" : `Sign in as ${who}` })
      .click();
  }

  test("the button signs in a bound person with a whole session and leaves nothing of Microsoft behind", async ({
    page,
    context,
  }) => {
    await page.goto(ssoUrl());
    const button = page.getByRole("button", { name: "Sign in with Microsoft" });
    await expect(button).toBeVisible();
    await button.click();
    await atMicrosoft(page, "bea");
    await expect(page.locator(".topbar")).toBeVisible();

    // The bearer token is in the session storage as after a password, the address is clean, and no
    // cookie of the flow is left.
    expect(page.url()).toBe(`${ssoUrl()}/`);
    const stored = await page.evaluate(() => ({
      token: sessionStorage.getItem("obop-token"),
      user: sessionStorage.getItem("obop-user"),
      all: JSON.stringify({ ...sessionStorage, ...localStorage }),
    }));
    expect(stored.token).toMatch(/^obop_[0-9a-f]{64}$/);
    expect(stored.user).toBe("bea");
    expect(stored.all, "no token of the provider in the browser").not.toMatch(
      /eyJ|e2e-access-token/,
    );
    expect((await context.cookies()).filter((c) => c.name.startsWith("obop_sso"))).toEqual([]);

    // The session works like any other.
    const mine = await page.request.get(`${ssoUrl()}/api/auth/mfa`, {
      headers: { authorization: `Bearer ${stored.token}` },
    });
    expect(mine.status()).toBe(200);
  });

  test("the request asks for openid and profile, and nothing else", async ({ request }) => {
    const scopes = (await (await request.get(`${idpUrl()}/control/scopes`)).json()) as string[];
    expect(scopes.length).toBeGreaterThan(0);
    for (const s of scopes) expect(s).toBe("openid profile");
  });

  test("a registration without the consent for profile gets no object id: a plain message here, the reason in the log", async ({
    page,
    request,
  }) => {
    await request.get(`${idpUrl()}/control?consent=0`);
    try {
      await page.goto(ssoUrl());
      await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
      await atMicrosoft(page, "bea");
      await expect(page.getByText(FAILED)).toBeVisible();
      await expect(page.locator(".topbar")).toHaveCount(0);
    } finally {
      await request.get(`${idpUrl()}/control?consent=1`);
    }
    // The administrator finds what to check in the log, not on the page.
    const log = await post(request, "/api/security_event/list", { limit: 1000 }, admin);
    const text = JSON.stringify(log.json);
    expect(text).toMatch(/no object id/);
    expect(text).toMatch(/profile scope/);
    expect(text).toMatch(/consent/);
  });

  test("a registration without the optional claims reports no second factor, and the log says so", async ({
    page,
    request,
  }) => {
    await request.get(`${idpUrl()}/control?optional_claims=0`);
    try {
      await page.goto(ssoUrl());
      await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
      await atMicrosoft(page, "bea");
      // An ordinary person still signs in (the policy asks nothing of them)...
      await expect(page.locator(".topbar")).toBeVisible();
    } finally {
      await request.get(`${idpUrl()}/control?optional_claims=1`);
    }
    // ...but Microsoft was not heard to say that a second factor was used.
    const log = await post(request, "/api/security_event/list", { limit: 1000 }, admin);
    expect(JSON.stringify(log.json)).toMatch(/does not report a second factor/);
  });

  test("a person who signs in with Microsoft only cannot use the password, and Microsoft still works", async ({
    page,
    request,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400, 401);
    // Bound is not tried: until the person has signed in once with the button, the administrator
    // is told to ask for that first (a wrong object id would otherwise be a lockout).
    const early = await post(
      request,
      "/api/user_account/set_sso_only",
      { id: "ada", sso_only: true },
      admin,
    );
    expect(early.status).toBe(400);
    expect(JSON.stringify(early.json)).toMatch(/never signed in/);
    await page.goto(ssoUrl());
    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "ada");
    await expect(page.locator(".topbar")).toBeVisible();
    await page.evaluate(() => sessionStorage.clear());
    const made = await post(
      request,
      "/api/user_account/set_sso_only",
      { id: "ada", sso_only: true },
      admin,
    );
    expect(made.status, JSON.stringify(made.json)).toBe(200);
    expect(made.json.value.sso_only).toBe(true);

    await page.goto(ssoUrl());
    await page.getByLabel("Username").fill(sso.users.ada.name);
    await page.getByLabel("Password").fill(sso.users.ada.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByText("Invalid username or password")).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);

    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "ada");
    await expect(page.locator(".topbar")).toBeVisible();
    expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe("ada");
  });

  test("a person with an authenticator app of their own is still asked for it", async ({
    page,
  }) => {
    await page.goto(ssoUrl());
    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "bob");
    await expect(page.getByRole("heading", { name: "Two-factor authentication" })).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);
    // The next step's code is accepted (one step of clock drift either way).
    await page.getByLabel("6-digit code").fill(code(bob!.secret, bob!.step + 1));
    await page.getByRole("button", { name: "Verify" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe("bob");
  });

  test("a person nobody is bound to, and a person who says no, get a plain message", async ({
    page,
  }) => {
    await page.goto(ssoUrl());
    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "stranger");
    await expect(page.getByText(FAILED)).toBeVisible();
    // Nothing says whether the person exists, and the address is clean.
    expect(page.url()).toBe(`${ssoUrl()}/`);
    await expect(page.locator(".topbar")).toHaveCount(0);
    expect(await page.evaluate(() => sessionStorage.getItem("obop-token"))).toBeNull();

    await page.getByRole("button", { name: "Sign in with Microsoft" }).click();
    await atMicrosoft(page, "cancel");
    await expect(page.getByText("The sign-in with Microsoft was cancelled.")).toBeVisible();
    // What Microsoft wrote about it is not shown.
    await expect(page.getByText(/AADSTS|consent/)).toHaveCount(0);
    expect(page.url()).toBe(`${ssoUrl()}/`);
  });

  /**
   * The round trip without a browser: the code of a callback that nobody has spent, with the
   * cookie jar of the browser that made it. (The page would exchange it at once.)
   */
  async function callbackCode(
    playwright: { request: { newContext(): Promise<APIRequestContext> } },
    who: string,
  ) {
    const jar = await playwright.request.newContext();
    const start = await jar.get(`${ssoUrl()}/api/auth/oidc/start`, { maxRedirects: 0 });
    expect(start.status()).toBe(303);
    const authorize = await jar.get(start.headers()["location"]!, { maxRedirects: 0 });
    const pick = new RegExp(`href="([^"]*who=${who})"`).exec(await authorize.text())?.[1];
    expect(pick, "the stand-in's page offers the person").toBeTruthy();
    const chosen = await jar.get(`${idpUrl()}${pick!.replace(/&amp;/g, "&")}`, { maxRedirects: 0 });
    const back = await jar.get(chosen.headers()["location"]!, { maxRedirects: 0 });
    const fragment = /#oidc=(obopsso_[0-9a-f]{64})$/.exec(back.headers()["location"] ?? "")?.[1];
    expect(fragment, `the callback redirected to ${back.headers()["location"]}`).toBeTruthy();
    return { jar, code: fragment! };
  }

  test("a one-time code is worth nothing to another browser, works once for the right one, and not twice", async ({
    playwright,
    browser,
    allowHttpErrors,
  }) => {
    allowHttpErrors(401);
    const exchange = (jar: APIRequestContext, code: string) =>
      jar.post(`${ssoUrl()}/api/auth/oidc/exchange`, { data: { code } });

    // Another browser (login CSRF) follows a link that carries a fresh, unspent code: it has not
    // the cookie of the browser that signed in, and gets a plain failure and no session.
    const lure = await callbackCode(playwright, "bea");
    const other = await browser.newContext();
    const stranger = await other.newPage();
    await stranger.goto(`${ssoUrl()}/#oidc=${lure.code}`);
    await expect(stranger.getByText(FAILED)).toBeVisible();
    await expect(stranger.locator(".topbar")).toHaveCount(0);
    expect(await stranger.evaluate(() => sessionStorage.getItem("obop-token"))).toBeNull();
    await other.close();
    await lure.jar.dispose();

    // The same at the API: the only difference to the next case is the browser.
    const victim = await callbackCode(playwright, "bea");
    const nobody = await playwright.request.newContext();
    expect((await exchange(nobody, victim.code)).status()).toBe(401);
    await nobody.dispose();

    // The right browser with its own fresh code: a session, and a replay of the same code gets none.
    const mine = await callbackCode(playwright, "bea");
    const first = await exchange(mine.jar, mine.code);
    expect(first.status()).toBe(200);
    expect(((await first.json()) as Json).token).toMatch(/^obop_[0-9a-f]{64}$/);
    // The done cookie is spent with the code: a replay needs both again, and has neither.
    const replay = await exchange(mine.jar, mine.code);
    expect(replay.status(), "a spent code is refused").toBe(401);
    await mine.jar.dispose();
    await victim.jar.dispose();
  });

  test("the sign-in page offers the button only where an administrator set it up", async ({
    page,
  }) => {
    // The ordinary instance of the suite has no Microsoft sign-in.
    await page.goto("/");
    await expect(page.getByLabel("Username")).toBeVisible();
    await expect(page.getByRole("button", { name: "Sign in with Microsoft" })).toHaveCount(0);
  });
});
