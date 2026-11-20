import { createHash, randomBytes } from "node:crypto";
import { request as playwrightRequest, type CDPSession, type Page } from "@playwright/test";
import { users, type UserName } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// E7c: passkeys in the autofill of the username field (conditional mediation) and a username-first
// sign-in for keys that do not keep the name (some hardware keys). Chrome's virtual authenticator
// stands in for the key; no real phone or security key was used. The instance is the passkey
// instance of tests/passkeys.spec.ts (reached as localhost, a valid relying party id); tess and
// quinn have no passkeys in any other spec there.
const origin = () => `http://localhost:${process.env.OBOP_E2E_PASSKEY_PORT}`;
const apiOrigin = () => `http://127.0.0.1:${process.env.OBOP_E2E_PASSKEY_PORT}`;
const CALLBACK = "http://127.0.0.1:53682/callback";
const SECURITY = "/#/customize/security?tab=security";

type Virtual = { cdp: CDPSession; id: string };

async function virtualAuthenticator(page: Page, resident: boolean): Promise<Virtual> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: resident ? "internal" : "usb",
      hasResidentKey: resident,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
  return { cdp, id: authenticatorId };
}

async function signInWithPassword(page: Page, user: UserName) {
  await page.goto(`${origin()}/`);
  await page.getByLabel("Username").fill(users[user].name);
  await page.getByLabel("Password").fill(users[user].password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
}

const form = (page: Page) => page.locator("form.tf-form");

/** Register an MCP client and return the address of its authorization page (the OAuth sign-in). */
async function authorizeUrl(): Promise<string> {
  const request = await playwrightRequest.newContext({ baseURL: apiOrigin() });
  const reg = await request.post("/oauth/register", {
    data: {
      client_name: "Autofill Claude",
      redirect_uris: [CALLBACK],
      token_endpoint_auth_method: "none",
    },
  });
  expect(reg.status()).toBe(201);
  const clientId = ((await reg.json()) as { client_id: string }).client_id;
  const challenge = createHash("sha256")
    .update(randomBytes(32).toString("base64url"))
    .digest("base64url");
  return `${origin()}/oauth/authorize?${new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: CALLBACK,
    state: "s1",
    code_challenge: challenge,
    code_challenge_method: "S256",
    resource: `${origin()}/mcp`,
  })}`;
}

/** Sign in, add a passkey and allow signing in with it alone. */
async function passkeyAlone(page: Page, user: UserName) {
  await signInWithPassword(page, user);
  await expect(page.locator(".topbar")).toBeVisible();
  await page.goto(`${origin()}${SECURITY}`);
  await expect(page.getByRole("heading", { name: /^Passkeys/ })).toBeVisible();
  await page.getByRole("button", { name: "Add a passkey" }).click();
  await form(page).getByLabel("Name").fill("Key");
  await form(page).getByLabel("Password").fill(users[user].password);
  await page.getByRole("button", { name: "Create passkey" }).click();
  await expect(page.getByRole("heading", { name: "Save your recovery codes" })).toBeVisible();
  await page.getByLabel("I have saved these codes").check();
  await page.getByRole("button", { name: "Done" }).click();
  await page.locator("label.switch", { hasText: "Let me sign in with a passkey alone" }).click();
  await form(page).getByLabel("Password").fill(users[user].password);
  await form(page).getByRole("button", { name: "Confirm with a passkey" }).click();
  await expect(form(page).getByText("Passkey confirmed.")).toBeVisible();
  await page.getByRole("button", { name: "Allow", exact: true }).click();
  await expect(page.getByText("Signing in with a passkey alone is on").first()).toBeVisible();
  await page.evaluate(() => sessionStorage.clear());
}

test.describe("passkey autofill and username-first (E7c)", () => {
  test("a browser with no authenticator says nothing about the offer", async ({ page }) => {
    await page.goto(`${origin()}/`);
    await expect(page.getByLabel("Username")).toHaveAttribute("autocomplete", "username webauthn");
    await page.waitForTimeout(1_500);
    await expect(page.locator(".notice")).toHaveCount(0);
  });

  test("the username field offers passkeys in its autofill, and picking one signs in", async ({
    page,
  }) => {
    test.setTimeout(90_000);
    await virtualAuthenticator(page, true);
    // Record what the page asks the browser for, before any script of the page runs.
    await page.addInitScript(() => {
      const w = window as unknown as { asked: string[] };
      w.asked = [];
      const get = navigator.credentials.get.bind(navigator.credentials);
      navigator.credentials.get = (o?: CredentialRequestOptions) => {
        w.asked.push(o?.mediation ?? "modal");
        return get(o);
      };
    });
    await passkeyAlone(page, "tess");
    await page.goto(`${origin()}/`);
    await expect(page.getByLabel("Username")).toHaveAttribute("autocomplete", "username webauthn");
    // The offer waits for the person; the virtual authenticator picks its one key for them.
    await expect(page.locator(".topbar")).toBeVisible();
    expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe(users.tess.name);
    expect(await page.evaluate(() => (window as unknown as { asked: string[] }).asked)).toContain(
      "conditional",
    );

    // The OAuth sign-in page of an MCP client offers the same autofill: picking the key goes on to
    // the consent page without a name or a button (the virtual authenticator answers the offer at
    // once, so the sign-in page is gone before it can be looked at).
    await page.goto(await authorizeUrl());
    await expect(page.getByRole("heading", { name: "Connect Autofill Claude?" })).toBeVisible();
    await expect(page.getByText(`Signed in as ${users.tess.name}.`)).toBeVisible();
  });

  test("a key that does not keep the name signs in after the name is typed", async ({
    page,
    allowHttpErrors,
  }) => {
    test.setTimeout(90_000);
    allowHttpErrors(401, 400);
    await virtualAuthenticator(page, false);
    await passkeyAlone(page, "quinn");

    // Without a name the browser has no key to offer: nothing signs in.
    await page.goto(`${origin()}/`);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.getByRole("alert").or(page.locator(".notice"))).toBeVisible();
    await expect(page.locator(".topbar")).toHaveCount(0);

    // A name that does not exist: the answer looks like any other, and the key is not asked.
    const unknown = await page.request.post(`${origin()}/api/auth/login/passkey/options`, {
      data: { username: "nobody-here" },
    });
    expect(unknown.status()).toBe(200);
    const ids = (await unknown.json()).publicKey.allowCredentials as { id: string }[];
    expect(ids.length).toBeGreaterThan(0);
    const again = await page.request.post(`${origin()}/api/auth/login/passkey/options`, {
      data: { username: "nobody-here" },
    });
    expect((await again.json()).publicKey.allowCredentials).toEqual(ids);

    // Another name typed: the browser has no key for it, and the page says to check the name.
    await page.getByLabel("Username").fill("nobody-x");
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.getByText("This device may have no passkey for that username")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.locator(".topbar")).toHaveCount(0);

    // The name typed: the key is asked for and signs in.
    await page.getByLabel("Username").fill(users.quinn.name);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe(users.quinn.name);

    // The OAuth sign-in page: the name typed goes with the passkey button, and the key that does
    // not keep the name signs in there too.
    await page.goto(await authorizeUrl());
    await page.getByLabel("Username").fill(users.quinn.name);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.getByRole("heading", { name: "Connect Autofill Claude?" })).toBeVisible();
    await expect(page.getByText(`Signed in as ${users.quinn.name}.`)).toBeVisible();
  });
});
