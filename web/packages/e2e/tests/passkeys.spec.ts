import { createHash, randomBytes } from "node:crypto";
import { request as playwrightRequest, type CDPSession, type Page } from "@playwright/test";
import { users, type UserName } from "../support/env.ts";
import { Api, expect, test, unnamedControls } from "../support/fixtures.ts";

// D65: passkeys (WebAuthn). Chrome's virtual authenticator (DevTools Protocol, WebAuthn domain)
// stands in for a phone or a security key: it holds real key pairs, signs real challenges and
// answers the browser's prompts without a person. Everything else is real: the back office, the
// server, the browser's own `navigator.credentials` and the sign-in pages of the OAuth flow.
//
// WebAuthn needs a host name, not an IP address, so the specs open a second instance (its public
// URL is localhost: a secure context and a valid relying party id) while the shared instance keeps
// the 127.0.0.1 address every other spec expects. Tests arrange data through its REST API.
const origin = () => `http://localhost:${process.env.OBOP_E2E_PASSKEY_PORT}`;
const apiOrigin = () => `http://127.0.0.1:${process.env.OBOP_E2E_PASSKEY_PORT}`;
const SECURITY = "/#/customize/security?tab=security";
const CALLBACK = "http://127.0.0.1:53682/callback";

type Virtual = { cdp: CDPSession; id: string; credentials: () => Promise<Credential[]> };
type Credential = { credentialId: string; signCount: number; isResidentCredential: boolean };

/** A software authenticator built into the browser: platform, resident keys, user verification. */
async function virtualAuthenticator(page: Page, userVerified = true): Promise<Virtual> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: userVerified,
      automaticPresenceSimulation: true,
    },
  });
  return {
    cdp,
    id: authenticatorId,
    credentials: async () =>
      (await cdp.send("WebAuthn.getCredentials", { authenticatorId })).credentials,
  };
}

async function signInForm(page: Page, user: UserName) {
  await page.goto(`${origin()}/`);
  await page.getByLabel("Username").fill(users[user].name);
  await page.getByLabel("Password").fill(users[user].password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
}

async function openSecurity(page: Page) {
  await page.goto(`${origin()}${SECURITY}`);
  await expect(page.getByRole("heading", { name: /^Passkeys/ })).toBeVisible();
}

/** The open form of the passkey panel (re-authentication and the fields of its action). */
const form = (page: Page) => page.locator("form.tf-form");

/** Type the password into the open re-authentication form. */
async function password(page: Page, user: UserName) {
  await form(page).getByLabel("Password").fill(users[user].password);
}

/** Add a passkey through the panel. Returns the recovery codes when it was the first second factor. */
async function addPasskey(page: Page, user: UserName, name: string): Promise<string[]> {
  await page.getByRole("button", { name: "Add a passkey" }).click();
  await form(page).getByLabel("Name").fill(name);
  await password(page, user);
  await page.getByRole("button", { name: "Create passkey" }).click();
  const heading = page.getByRole("heading", { name: "Save your recovery codes" });
  const first = await heading
    .waitFor({ timeout: 5_000 })
    .then(() => true)
    .catch(() => false);
  if (!first) {
    await expect(page.getByText("Passkey added").first()).toBeVisible();
    return [];
  }
  const codes = await page
    .getByRole("list", { name: "Recovery codes" })
    .locator("code")
    .allTextContents();
  await page.getByLabel("I have saved these codes").check();
  await page.getByRole("button", { name: "Done" }).click();
  return codes;
}

/** Re-authenticate an open form with the passkey and the password. */
async function confirmWithPasskey(page: Page, user: UserName) {
  await password(page, user);
  await form(page).getByRole("button", { name: "Confirm with a passkey" }).click();
  await expect(form(page).getByText("Passkey confirmed.")).toBeVisible();
}

/** Register a client, as any MCP client does, and return the address of its authorization page. */
async function authorizeUrl(): Promise<string> {
  const request = await playwrightRequest.newContext({ baseURL: apiOrigin() });
  const reg = await request.post("/oauth/register", {
    data: {
      client_name: "Passkey Claude",
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

const row = (page: Page, name: string) =>
  page.getByRole("list", { name: "Your passkeys" }).getByRole("listitem").filter({ hasText: name });

test.describe("passkeys", () => {
  test("enrol, use as the second step, rename and remove", async ({ page, allowHttpErrors }) => {
    test.setTimeout(90_000);
    allowHttpErrors(401, 400);
    const http = await playwrightRequest.newContext({ baseURL: apiOrigin() });
    const api = new Api(http);
    const authenticator = await virtualAuthenticator(page);
    await signInForm(page, "pia");
    await expect(page.locator(".topbar")).toBeVisible();
    await openSecurity(page);
    await expect(page.getByText("None", { exact: true })).toBeVisible();

    // A wrong password stops the ceremony before the browser is asked anything.
    await page.getByRole("button", { name: "Add a passkey" }).click();
    await form(page).getByLabel("Name").fill("Work laptop");
    await form(page).getByLabel("Password").fill("not the password");
    await page.getByRole("button", { name: "Create passkey" }).click();
    await expect(page.getByText("That password is not right.")).toBeVisible();
    expect(await authenticator.credentials()).toHaveLength(0);
    await page.getByRole("button", { name: "Cancel" }).click();

    // Enrol: the first second factor also gives recovery codes, once.
    const codes = await addPasskey(page, "pia", "Work laptop");
    expect(codes).toHaveLength(10);
    await expect(row(page, "Work laptop")).toBeVisible();
    await expect(row(page, "Work laptop")).toContainText("Never used");
    await expect(row(page, "Work laptop")).toContainText("Verifies you");
    const [made] = await authenticator.credentials();
    expect(made?.isResidentCredential).toBe(true);

    // The password alone no longer opens a session; the passkey completes it.
    const first = await api.post("/api/auth/login", {
      username: users.pia.name,
      password: users.pia.password,
    });
    expect(first.json.mfa_required).toBe(true);
    expect(first.json.methods).toEqual(["passkey"]);
    expect(first.json.token).toBeUndefined();

    await page.evaluate(() => sessionStorage.clear());
    await signInForm(page, "pia");
    await expect(page.getByRole("heading", { name: "Two-factor authentication" })).toBeVisible();
    await expect(page.getByLabel("6-digit code")).toHaveCount(0);
    await page.getByRole("button", { name: "Use a passkey" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    const [used] = await authenticator.credentials();
    expect(used!.signCount).toBeGreaterThan(made!.signCount);

    // Renaming asks for the password and a second factor: here the passkey itself.
    await openSecurity(page);
    await row(page, "Work laptop").getByRole("button", { name: "Rename" }).click();
    await expect(form(page).getByLabel("Name")).toHaveValue("Work laptop");
    await form(page).getByLabel("Name").fill("Office laptop");
    await password(page, "pia");
    // Not yet: the passkey has to confirm it first.
    await expect(page.getByRole("button", { name: "Rename", exact: true })).toBeDisabled();
    await confirmWithPasskey(page, "pia");
    await page.getByRole("button", { name: "Rename", exact: true }).click();
    await expect(row(page, "Office laptop")).toBeVisible();
    await expect(row(page, "Office laptop")).toContainText("Last used");

    // The log says what happened and nothing that could be used as a credential.
    // (The person sees their own events through the administrator's log only; check the API.)
    const root = await api.token("root");
    const log = await api.post("/api/security_event/list", {}, root);
    const kinds = (log.json.value as { user: string; kind: string; detail: string }[])
      .filter((e) => e.user === users.pia.name)
      .map((e) => e.kind);
    for (const k of ["passkey_register", "passkey_used", "passkey_rename"])
      expect(kinds, k).toContain(k);

    // Remove it: another proof, then it is gone and the password alone works again.
    await row(page, "Office laptop").getByRole("button", { name: "Remove" }).click();
    await confirmWithPasskey(page, "pia");
    await page.getByRole("button", { name: "Remove", exact: true }).click();
    await expect(page.getByText("None", { exact: true })).toBeVisible();
    const again = await api.post("/api/auth/login", {
      username: users.pia.name,
      password: users.pia.password,
    });
    expect(again.json.token).toMatch(/^obop_/);
    // The authenticator still holds the key, but the server no longer knows it.
    expect(await authenticator.credentials()).toHaveLength(1);
  });

  test("sign in with a passkey alone once the person allows it", async ({
    page,
    allowHttpErrors,
  }) => {
    test.setTimeout(90_000);
    allowHttpErrors(401);
    await virtualAuthenticator(page);
    await signInForm(page, "pax");
    await expect(page.locator(".topbar")).toBeVisible();
    await openSecurity(page);
    await addPasskey(page, "pax", "Phone");

    // Not allowed yet: the passkey alone is refused, with a message that points back to the password.
    await page.evaluate(() => sessionStorage.clear());
    await page.goto(`${origin()}/`);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.getByText(/could not be used to sign in/)).toBeVisible();

    // Sign in the long way, allow it, and the short way works.
    await signInForm(page, "pax");
    await page.getByRole("button", { name: "Use a passkey" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    await openSecurity(page);
    await page.locator("label.switch", { hasText: "Let me sign in with a passkey alone" }).click();
    await confirmWithPasskey(page, "pax");
    await page.getByRole("button", { name: "Allow", exact: true }).click();
    await expect(page.getByText("Signing in with a passkey alone is on").first()).toBeVisible();

    await page.evaluate(() => sessionStorage.clear());
    await page.goto(`${origin()}/`);
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.locator(".topbar")).toBeVisible();
    expect(await page.evaluate(() => sessionStorage.getItem("obop-user"))).toBe(users.pax.name);

    // The authorization page of an MCP client offers the same, with no username to type.
    await page.goto(await authorizeUrl());
    await page.getByRole("button", { name: "Sign in with a passkey" }).click();
    await expect(page.getByRole("heading", { name: "Connect Passkey Claude?" })).toBeVisible();
    await expect(page.getByText("Signed in as pax.")).toBeVisible();
  });

  test("the sign-in page of an OAuth connection takes a passkey, and its policy stays strict", async ({
    page,
    allowHttpErrors,
  }) => {
    test.setTimeout(90_000);
    allowHttpErrors(401);
    await virtualAuthenticator(page);
    await signInForm(page, "pom");
    await expect(page.locator(".topbar")).toBeVisible();
    await openSecurity(page);
    await addPasskey(page, "pom", "Tablet");

    // A client registers (as any MCP client does) and sends the person to the authorization page.
    const authorize = await authorizeUrl();

    // The page has one script and a policy that names it by hash and nothing else.
    const response = await page.goto(authorize);
    const csp = response!.headers()["content-security-policy"]!;
    expect(csp).toMatch(/script-src 'sha256-[A-Za-z0-9+/=]+'; connect-src 'self';/);
    expect(csp).toContain("default-src 'none'");
    expect(csp).not.toMatch(/unsafe|nonce/);
    expect(await page.locator("script").count()).toBe(1);
    await expect(page.getByRole("heading", { name: "Sign in to continue" })).toBeVisible();
    expect(await unnamedControls(page)).toEqual([]);

    // Password, then the second step is a passkey.
    await page.getByLabel("Username").fill(users.pom.name);
    await page.getByLabel("Password").fill(users.pom.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Two-factor code" })).toBeVisible();
    await page.getByRole("button", { name: "Use a passkey instead" }).click();
    await expect(page.getByRole("heading", { name: "Connect Passkey Claude?" })).toBeVisible();
    await expect(page.getByText("Signed in as pom.")).toBeVisible();
  });
});
