import { createHash, randomBytes } from "node:crypto";
import type { APIRequestContext, Page } from "@playwright/test";
import { users } from "../support/env.ts";
import { expect, test, unnamedControls } from "../support/fixtures.ts";

// D64: an MCP client (Claude Desktop, claude.ai, Claude Code) connects with a browser sign-in. The
// browser half is real: the consent page of the instance, the form posts and the redirect back to
// the client. The client half is played by the test (registration, PKCE, code exchange). The
// callback address is a loopback port nothing listens on; the test answers it itself.
const CALLBACK = "http://127.0.0.1:53682/callback";
const SECURITY = "/customize/security?tab=security";

function pkce() {
  const verifier = randomBytes(32).toString("base64url");
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  return { verifier, challenge };
}

async function registerClient(request: APIRequestContext, name: string): Promise<string> {
  const res = await request.post("/oauth/register", {
    data: { client_name: name, redirect_uris: [CALLBACK], token_endpoint_auth_method: "none" },
  });
  expect(res.status()).toBe(201);
  return (await res.json()).client_id as string;
}

function authorizeUrl(clientId: string, challenge: string, resource: string, extra = {}) {
  return `/oauth/authorize?${new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: CALLBACK,
    state: "state-1",
    code_challenge: challenge,
    code_challenge_method: "S256",
    resource,
    ...extra,
  })}`;
}

/** Answer the callback ourselves and remember where the browser was sent. */
async function catchCallback(page: Page): Promise<() => Promise<URL>> {
  const arrived = new Promise<URL>((resolve) => {
    void page.route(`${new URL(CALLBACK).origin}/**`, async (route) => {
      resolve(new URL(route.request().url()));
      // 204: the browser stays on the page it was on, so no document (and no favicon) is fetched.
      await route.fulfill({ status: 204 });
    });
  });
  return () => arrived;
}

async function mcpStatus(request: APIRequestContext, token: string) {
  const res = await request.post("/mcp", {
    headers: {
      authorization: `Bearer ${token}`,
      accept: "application/json, text/event-stream",
      "content-type": "application/json",
    },
    data: {
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-11-25",
        capabilities: {},
        clientInfo: { name: "obop-e2e", version: "0" },
      },
    },
  });
  return { status: res.status(), session: res.headers()["mcp-session-id"] ?? "" };
}

async function toolNames(request: APIRequestContext, token: string): Promise<string[]> {
  const { session } = await mcpStatus(request, token);
  const res = await request.post("/mcp", {
    headers: {
      authorization: `Bearer ${token}`,
      accept: "application/json, text/event-stream",
      "content-type": "application/json",
      "mcp-session-id": session,
    },
    data: { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
  });
  expect(res.status()).toBe(200);
  return ((await res.json()).result.tools as { name: string }[]).map((t) => t.name);
}

const HELD = ["base.user", "crm.user", "sales.user", "product.user"];

/**
 * One trip through the browser: sign in as sam, narrow the groups to `only`, allow. Returns the
 * code the browser carried back and the verifier that belongs to it. With `inspect` it also
 * checks what the pages show.
 */
async function connect(
  page: Page,
  request: APIRequestContext,
  clientId: string,
  resource: string,
  opts: { only?: string[]; days?: string; inspect?: boolean } = {},
) {
  const { verifier, challenge } = pkce();
  await page.unroute(`${new URL(CALLBACK).origin}/**`);
  const callback = await catchCallback(page);
  await page.goto(authorizeUrl(clientId, challenge, resource));
  await expect(page.getByRole("heading", { name: "Sign in to continue" })).toBeVisible();
  if (opts.inspect) {
    await expect(page.getByText("E2E Claude wants to connect")).toBeVisible();
    expect(await unnamedControls(page)).toEqual([]);
    await page.getByLabel("Username").fill(users.sam.name);
    await page.getByLabel("Password").fill("not the password");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("alert")).toContainText("not right");
  }
  await page.getByLabel("Username").fill(users.sam.name);
  await page.getByLabel("Password").fill(users.sam.password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByRole("heading", { name: "Connect E2E Claude?" })).toBeVisible();
  const groups = page.getByRole("group", { name: /permission groups/ });
  if (opts.inspect) {
    await expect(page.getByText("Signed in as sam.")).toBeVisible();
    await expect(page.getByText("127.0.0.1:53682").first()).toBeVisible();
    await expect(page.getByText("cannot approve its own proposals")).toBeVisible();
    expect(await unnamedControls(page)).toEqual([]);
    // Only the groups the person holds are offered, and all start ticked.
    await expect(groups.getByRole("checkbox")).toHaveCount(HELD.length);
    for (const g of HELD) await expect(groups.getByRole("checkbox", { name: g })).toBeChecked();
  }
  for (const g of HELD.filter((g) => opts.only && !opts.only.includes(g)))
    await groups.getByRole("checkbox", { name: g }).uncheck();
  if (opts.days) await page.getByLabel("Access ends after").selectOption({ label: opts.days });
  await page.getByRole("button", { name: "Allow" }).click();
  const back = await callback();
  if (opts.inspect) {
    // The browser went back to the client with the code and the state it sent.
    expect(back.searchParams.get("state")).toBe("state-1");
    expect(back.searchParams.get("iss")).toBe(new URL(resource).origin);
  }
  const code = back.searchParams.get("code")!;
  expect(code).toMatch(/^oac_/);
  return { code, verifier };
}

test.describe("MCP authorization (OAuth)", () => {
  test("a client connects with a browser sign-in, the person narrows it, and revoking ends it", async ({
    page,
    request,
    api,
    signIn,
    baseURL,
  }) => {
    test.setTimeout(120_000);
    const resource = `${baseURL}/mcp`;
    const clientId = await registerClient(request, "E2E Claude");
    const form = {
      grant_type: "authorization_code",
      client_id: clientId,
      redirect_uri: CALLBACK,
      resource,
    };
    const exchange = (code: string, code_verifier: string) =>
      request.post("/oauth/token", { form: { ...form, code, code_verifier } });
    const refresh = (refresh_token: string) =>
      request.post("/oauth/token", {
        form: { grant_type: "refresh_token", client_id: clientId, refresh_token },
      });

    // A wrong verifier is refused, and the code is burnt by the attempt.
    const first = await connect(page, request, clientId, resource, {
      only: ["sales.user"],
      days: "7 days",
      inspect: true,
    });
    expect((await exchange(first.code, randomBytes(32).toString("base64url"))).status()).toBe(400);
    expect((await exchange(first.code, first.verifier)).status()).toBe(400);

    // A code works once; using it again ends the connection it produced.
    const second = await connect(page, request, clientId, resource, { only: ["sales.user"] });
    const ok = await exchange(second.code, second.verifier);
    expect(ok.status()).toBe(200);
    expect(ok.headers()["cache-control"]).toBe("no-store");
    const t2 = await ok.json();
    expect(t2).toMatchObject({ token_type: "Bearer", expires_in: 3600, scope: "sales.user" });
    expect((await mcpStatus(request, t2.access_token)).status).toBe(200);
    expect((await exchange(second.code, second.verifier)).status()).toBe(400);
    expect((await mcpStatus(request, t2.access_token)).status).toBe(401);

    // The token works on /mcp with fewer tools than sam has, and it is not a REST token.
    const third = await connect(page, request, clientId, resource, { only: ["sales.user"] });
    const t3 = await (await exchange(third.code, third.verifier)).json();
    const samTools = await toolNames(request, await api.token("sam"));
    const appTools = await toolNames(request, t3.access_token);
    expect(appTools.length).toBeGreaterThan(0);
    expect(appTools.length).toBeLessThan(samTools.length);
    const meta = await request.get("/api/meta", {
      headers: { authorization: `Bearer ${t3.access_token}` },
    });
    expect(meta.status()).toBe(401);

    // Refresh tokens rotate; replaying the first one ends the whole connection.
    const rotated = await refresh(t3.refresh_token);
    expect(rotated.status()).toBe(200);
    const t4 = await rotated.json();
    expect(t4.refresh_token).not.toBe(t3.refresh_token);
    expect((await mcpStatus(request, t3.access_token)).status).toBe(401);
    expect((await mcpStatus(request, t4.access_token)).status).toBe(200);
    expect((await refresh(t3.refresh_token)).status()).toBe(400);
    expect((await mcpStatus(request, t4.access_token)).status).toBe(401);

    // A fresh connection; the person revokes it in Customize > Security > Connected apps.
    const fifth = await connect(page, request, clientId, resource);
    const t5 = await (await exchange(fifth.code, fifth.verifier)).json();
    expect((await mcpStatus(request, t5.access_token)).status).toBe(200);
    await page.unroute(`${new URL(CALLBACK).origin}/**`);
    await signIn("sam", SECURITY);
    const apps = page.getByRole("table", { name: "Connected apps" });
    await expect(apps).toBeVisible();
    await expect(apps.getByText("E2E Claude")).toHaveCount(1);
    await expect(apps.getByText("127.0.0.1:53682")).toBeVisible();
    expect(await apps.textContent()).not.toContain(t5.access_token);
    expect(await apps.textContent()).not.toContain(t5.refresh_token);
    await apps.getByRole("button", { name: "Revoke E2E Claude" }).click();
    await expect(page.getByText("E2E Claude can no longer act for you.")).toBeVisible();
    await expect(page.getByRole("table", { name: "Connected apps" })).toHaveCount(0);
    await expect(page.getByText("No apps are connected.")).toBeVisible();
    expect((await mcpStatus(request, t5.access_token)).status).toBe(401);
    expect((await refresh(t5.refresh_token)).status()).toBe(400);
  });

  test("denying sends access_denied; a forged redirect gets a page, never a redirect", async ({
    page,
    request,
    baseURL,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const resource = `${baseURL}/mcp`;
    const clientId = await registerClient(request, "E2E refused");
    const { challenge } = pkce();
    const callback = await catchCallback(page);
    await page.goto(authorizeUrl(clientId, challenge, resource));
    await page.getByLabel("Username").fill(users.sam.name);
    await page.getByLabel("Password").fill(users.sam.password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("button", { name: "Deny" }).click();
    const back = await callback();
    expect(back.searchParams.get("error")).toBe("access_denied");
    expect(back.searchParams.get("state")).toBe("state-1");
    expect(back.searchParams.get("code")).toBeNull();

    // A redirect address the client did not register is never followed.
    await page.unroute(`${new URL(CALLBACK).origin}/**`);
    await page.goto(
      authorizeUrl(clientId, challenge, resource, { redirect_uri: "https://evil.example/cb" }),
    );
    await expect(page.getByRole("heading", { name: "This cannot continue" })).toBeVisible();
    await expect(page.getByText("not one the application registered")).toBeVisible();
    expect(new URL(page.url()).origin).toBe(new URL(baseURL!).origin);
  });

  test("the consent page speaks the language of the browser and fits a phone", async ({
    page,
    request,
    baseURL,
  }) => {
    const clientId = await registerClient(request, "E2E Sprache");
    const { challenge } = pkce();
    await page.setViewportSize({ width: 375, height: 700 });
    await page.goto(authorizeUrl(clientId, challenge, `${baseURL}/mcp`, { lang: "de" }));
    await expect(page.getByRole("heading", { name: "Zum Fortfahren anmelden" })).toBeVisible();
    await page.getByLabel("Benutzername").fill(users.sam.name);
    await page.getByLabel("Passwort").fill(users.sam.password);
    await page.getByRole("button", { name: "Anmelden" }).click();
    await expect(page.getByRole("heading", { name: "E2E Sprache verbinden?" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Erlauben" })).toBeVisible();
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(0);
    await page.getByRole("button", { name: "Ablehnen" }).click();
  });
});
