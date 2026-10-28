import { createHash, randomBytes } from "node:crypto";
import type { APIRequestContext } from "@playwright/test";
import { listenForCallback, type Callback } from "../support/callback.ts";
import { users } from "../support/env.ts";
import { expect, test as base, unnamedControls } from "../support/fixtures.ts";

// The D64 amendment: a native app signs in through the system browser (here: the consent page of
// the instance in the test browser, and a real loopback listener that receives the redirect, the
// way a mobile app does on RFC 8252's loopback redirect), gets an access token for /api, and the
// person sees the device in Customize > Security > Devices, renames it and revokes it. The app
// half (registration, PKCE, the code exchange, the calls) is played by the test.
const test = base.extend<{ callback: Callback }>({
  // eslint-disable-next-line no-empty-pattern -- Playwright needs a destructured first argument
  callback: async ({}, use) => {
    const callback = await listenForCallback();
    await use(callback);
    await callback.close();
  },
});
const SECURITY = "/customize/security?tab=security";

function pkce() {
  const verifier = randomBytes(32).toString("base64url");
  return { verifier, challenge: createHash("sha256").update(verifier).digest("base64url") };
}

async function registerMobile(request: APIRequestContext, redirect: string): Promise<string> {
  const res = await request.post("/oauth/register", {
    data: {
      client_name: "E2E Mileage",
      obop_client_class: "mobile",
      redirect_uris: [redirect],
      token_endpoint_auth_method: "none",
    },
  });
  expect(res.status()).toBe(201);
  expect((await res.json()).obop_client_class).toBe("mobile");
  return (await res.json()).client_id as string;
}

test.describe("Mobile apps: sign-in and devices", () => {
  test("an app signs in, the person sees the device, renames it and revokes it", async ({
    page,
    request,
    callback,
    signIn,
    baseURL,
  }) => {
    test.setTimeout(120_000);
    const resource = `${baseURL}/api`;
    const clientId = await registerMobile(request, callback.url);

    // A redirect that is not a mobile redirect is refused at registration.
    for (const bad of ["http://example.com/cb", "http://localhost:3000/cb", "claude://callback"]) {
      const res = await request.post("/oauth/register", {
        data: { obop_client_class: "mobile", redirect_uris: [bad] },
      });
      expect(res.status(), bad).toBe(400);
      expect((await res.json()).error).toBe("invalid_redirect_uri");
    }

    // The browser: the consent page says it is a mobile app and offers the groups of sam.
    const { verifier, challenge } = pkce();
    await page.goto(
      `/oauth/authorize?${new URLSearchParams({
        response_type: "code",
        client_id: clientId,
        redirect_uri: callback.url,
        state: "state-1",
        code_challenge: challenge,
        code_challenge_method: "S256",
        resource,
      })}`,
    );
    await page.getByLabel("Username").fill(users.sam.name);
    await page.getByLabel("Password").fill(users.sam.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Connect E2E Mileage?" })).toBeVisible();
    await expect(page.getByText("is a mobile app that registered itself")).toBeVisible();
    await expect(page.getByText("Customize, Security, Devices")).toBeVisible();
    expect(await unnamedControls(page)).toEqual([]);
    const groups = page.getByRole("group", { name: /permission groups/ });
    await expect(groups.getByRole("checkbox", { name: "base.user" })).toBeChecked();
    await groups.getByRole("checkbox", { name: "crm.user" }).uncheck();
    await page.getByRole("button", { name: "Allow" }).click();
    const back = await callback.next();
    expect(back.searchParams.get("state")).toBe("state-1");
    const code = back.searchParams.get("code")!;
    expect(code).toMatch(/^oac_/);

    // The app exchanges the code and says what the device is.
    const exchange = await request.post("/oauth/token", {
      form: {
        grant_type: "authorization_code",
        client_id: clientId,
        redirect_uri: callback.url,
        code,
        code_verifier: verifier,
        resource,
        device_name: "E2E Pixel",
        device_platform: "android",
        device_app_version: "1.4.2",
      },
    });
    expect(exchange.status()).toBe(200);
    const t = await exchange.json();
    expect(t.scope).toContain("base.user");
    expect(t.scope).not.toContain("crm.user");
    const bearer = { authorization: `Bearer ${t.access_token}` };

    // /api takes the token, /mcp and the account routes do not.
    const list = await request.post("/api/party/list", { headers: bearer, data: {} });
    expect(list.status()).toBe(200);
    expect(
      (await request.post("/api/crm_lead/list", { headers: bearer, data: {} })).status(),
    ).not.toBe(200);
    const mcp = await request.post("/mcp", {
      headers: {
        ...bearer,
        accept: "application/json, text/event-stream",
        "content-type": "application/json",
      },
      data: { jsonrpc: "2.0", id: 1, method: "ping" },
    });
    expect(mcp.status()).toBe(401);
    expect((await request.get("/api/auth/oauth/grants", { headers: bearer })).status()).toBe(401);
    expect((await request.get("/api/auth/sessions", { headers: bearer })).status()).toBe(404);

    // The person: the device is listed under Devices, not under Connected apps.
    await signIn("sam", SECURITY);
    const devices = page.getByRole("table", { name: "Devices" });
    await expect(devices).toBeVisible();
    const row = devices.getByRole("row", { name: /E2E Pixel/ });
    await expect(row).toHaveCount(1);
    await expect(row).toContainText("android 1.4.2");
    await expect(row).toContainText("E2E Mileage");
    await expect(row).toContainText("Signed in");
    expect(await devices.textContent()).not.toContain(t.access_token);
    expect(await devices.textContent()).not.toContain(t.refresh_token);
    await expect(page.getByRole("table", { name: "Connected apps" })).toHaveCount(0);
    expect(await unnamedControls(page)).toEqual([]);

    // Rename.
    await row.getByRole("button", { name: "Rename E2E Pixel" }).click();
    await page.getByLabel("Device name").fill("Work phone");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(page.getByText("Device renamed.")).toBeVisible();
    const renamed = devices.getByRole("row", { name: /Work phone/ });
    await expect(renamed).toHaveCount(1);

    // Revoke: the app is out at once.
    await renamed.getByRole("button", { name: "Revoke Work phone" }).click();
    await expect(page.getByText("Work phone is signed out.")).toBeVisible();
    await expect(devices.getByRole("row", { name: /Work phone/ })).toContainText("Revoked");
    await expect(devices.getByRole("button", { name: /Revoke Work phone/ })).toHaveCount(0);
    expect((await request.post("/api/party/list", { headers: bearer, data: {} })).status()).toBe(
      401,
    );
    const refresh = await request.post("/oauth/token", {
      form: { grant_type: "refresh_token", client_id: clientId, refresh_token: t.refresh_token },
    });
    expect(refresh.status()).toBe(400);
    expect((await refresh.json()).error).toBe("invalid_grant");

    // The security log of the administrator names the three events and no secret.
    await signIn("root", SECURITY);
    const log = page.getByRole("table", { name: "Security log" });
    await expect(log).toContainText("Device signed in");
    await expect(log).toContainText("Device renamed");
    await expect(log).toContainText("Device signed out");
    expect(await log.textContent()).not.toContain(t.refresh_token);
  });

  test("a person without a device sees a hint, and what is not theirs is not listed", async ({
    page,
    signIn,
  }) => {
    await signIn("dana", SECURITY);
    await expect(page.getByRole("heading", { name: "Devices" })).toBeVisible();
    await expect(page.getByText("No device is signed in.")).toBeVisible();
    await expect(page.getByRole("table", { name: "Devices" })).toHaveCount(0);
  });
});
