import { createHash, randomBytes } from "node:crypto";
import type { APIRequestContext, Page } from "@playwright/test";
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

/** Register a mobile client; `scope` is the groups it declares (it is offered nothing else). */
async function registerMobile(
  request: APIRequestContext,
  redirect: string,
  scope?: string,
): Promise<string> {
  const res = await request.post("/oauth/register", {
    data: {
      client_name: "E2E Mileage",
      obop_client_class: "mobile",
      redirect_uris: [redirect],
      token_endpoint_auth_method: "none",
      ...(scope === undefined ? {} : { scope }),
    },
  });
  expect(res.status()).toBe(201);
  expect((await res.json()).obop_client_class).toBe("mobile");
  return (await res.json()).client_id as string;
}

function authorizeUrl(
  clientId: string,
  redirect: string,
  resource: string,
  challenge: string,
  scope?: string,
) {
  return `/oauth/authorize?${new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: redirect,
    state: "state-1",
    code_challenge: challenge,
    code_challenge_method: "S256",
    resource,
    ...(scope === undefined ? {} : { scope }),
  })}`;
}

async function signInAtTheConsentPage(page: Page, user: { name: string; password: string }) {
  await page.getByLabel("Username").fill(user.name);
  await page.getByLabel("Password").fill(user.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Connect E2E Mileage?" })).toBeVisible();
}

/** An app that asks for `base.user`, signs `sam` in on the consent page and gets its tokens. */
async function appSignsIn(
  page: Page,
  request: APIRequestContext,
  callback: Callback,
  baseURL: string,
  device: string,
) {
  const resource = `${baseURL}/api`;
  const clientId = await registerMobile(request, callback.url, "base.user");
  const { verifier, challenge } = pkce();
  await page.goto(authorizeUrl(clientId, callback.url, resource, challenge, "base.user"));
  await signInAtTheConsentPage(page, users.sam);
  const groups = page.getByRole("group", { name: /permission groups/ });
  // The app asked for this one in its request, so it is ticked; nothing else is offered.
  await expect(groups.getByRole("checkbox", { name: "base.user" })).toBeChecked();
  await expect(groups.getByRole("checkbox")).toHaveCount(1);
  await page.getByRole("button", { name: "Allow" }).click();
  const code = (await callback.next()).searchParams.get("code")!;
  const exchange = await request.post("/oauth/token", {
    form: {
      grant_type: "authorization_code",
      client_id: clientId,
      redirect_uri: callback.url,
      code,
      code_verifier: verifier,
      resource,
      device_name: device,
      device_platform: "android",
      device_app_version: "1.4.2",
    },
  });
  expect(exchange.status()).toBe(200);
  return { clientId, tokens: await exchange.json() };
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
    const clientId = await registerMobile(request, callback.url, "base.user crm.user");

    // A redirect that is not a mobile redirect is refused at registration: a web address of any
    // kind, a name that can be pointed elsewhere, a scheme anybody can claim.
    for (const bad of [
      "http://example.com/cb",
      "http://localhost:3000/cb",
      "claude://callback",
      "https://app.example.test/mileage/callback",
    ]) {
      const res = await request.post("/oauth/register", {
        data: { obop_client_class: "mobile", redirect_uris: [bad] },
      });
      expect(res.status(), bad).toBe(400);
      expect((await res.json()).error).toBe("invalid_redirect_uri");
    }
    // And so is a declaration of a group a phone may never have.
    const admin = await request.post("/oauth/register", {
      data: {
        obop_client_class: "mobile",
        redirect_uris: [callback.url],
        scope: "base.user platform.admin",
      },
    });
    expect(admin.status()).toBe(400);
    expect((await admin.json()).error).toBe("invalid_client_metadata");

    // The browser: the consent page says it is a mobile app and where the result goes, offers the
    // groups it declared that sam holds (sam holds more), and ticks none of them.
    const { verifier, challenge } = pkce();
    await page.goto(authorizeUrl(clientId, callback.url, resource, challenge));
    await signInAtTheConsentPage(page, users.sam);
    await expect(page.getByText("is a mobile app that registered itself")).toBeVisible();
    await expect(page.getByText("a program running on this computer")).toBeVisible();
    await expect(page.getByText("Customize, Security, Devices")).toBeVisible();
    expect(await unnamedControls(page)).toEqual([]);
    const groups = page.getByRole("group", { name: /permission groups/ });
    await expect(groups.getByRole("checkbox")).toHaveCount(2);
    await expect(groups.getByRole("checkbox", { name: "base.user" })).not.toBeChecked();
    await expect(groups.getByRole("checkbox", { name: "crm.user" })).not.toBeChecked();
    await expect(groups.getByRole("checkbox", { name: "sales.user" })).toHaveCount(0);
    await groups.getByRole("checkbox", { name: "base.user" }).check();
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
    expect(t.scope).toBe("base.user");
    expect(t.device_id).toBeTruthy();
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
    // The app sees its own device by the id it was given, and lists no others.
    const own = await request.post("/api/device/get", {
      headers: bearer,
      data: { id: t.device_id },
    });
    expect(own.status()).toBe(200);
    expect((await own.json()).value.name).toBe("E2E Pixel");
    const listed = await request.post("/api/device/list", { headers: bearer, data: {} });
    expect(listed.status()).toBe(400);
    expect((await listed.json()).error).toContain("cannot be listed");

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

    // The security log of the administrator names the three events and no secret. A new document
    // first: the page keeps the session it started with when only the address bar changes.
    await page.goto("/healthz");
    await signIn("root", SECURITY);
    const log = page.getByRole("table", { name: "Security log" });
    await expect(log).toContainText("Device signed in");
    await expect(log).toContainText("Device renamed");
    await expect(log).toContainText("Device signed out");
    expect(await log.textContent()).not.toContain(t.refresh_token);
  });

  test("an app that declares no group is offered none and signs in with none", async ({
    page,
    request,
    callback,
    baseURL,
  }) => {
    const resource = `${baseURL}/api`;
    const clientId = await registerMobile(request, callback.url);
    const { verifier, challenge } = pkce();
    await page.goto(authorizeUrl(clientId, callback.url, resource, challenge));
    await signInAtTheConsentPage(page, users.sam);
    await expect(page.getByText("asked for no permission groups")).toBeVisible();
    await expect(page.getByRole("checkbox")).toHaveCount(0);
    await page.getByRole("button", { name: "Allow" }).click();
    const code = (await callback.next()).searchParams.get("code")!;
    const exchange = await request.post("/oauth/token", {
      form: {
        grant_type: "authorization_code",
        client_id: clientId,
        redirect_uri: callback.url,
        code,
        code_verifier: verifier,
        resource,
      },
    });
    expect(exchange.status()).toBe(200);
    const t = await exchange.json();
    expect(t.scope).toBe("");
    const bearer = { authorization: `Bearer ${t.access_token}` };
    // No group, so no data: the commands that need one say no.
    expect(
      (await request.post("/api/party/list", { headers: bearer, data: {} })).status(),
    ).not.toBe(200);
    // It can still see and end its own device.
    const own = await request.post("/api/device/revoke", {
      headers: bearer,
      data: { id: t.device_id },
    });
    expect(own.status()).toBe(200);
  });

  test("a person with no device sees a hint", async ({ page, signIn }) => {
    await signIn("dana", SECURITY);
    await expect(page.getByRole("heading", { name: "Devices" })).toBeVisible();
    await expect(page.getByText("No device is signed in.")).toBeVisible();
    await expect(page.getByRole("table", { name: "Devices" })).toHaveCount(0);
  });

  test("an administrator sees everybody's devices with their owners and renames none of them", async ({
    page,
    request,
    callback,
    signIn,
    baseURL,
  }) => {
    test.setTimeout(120_000);
    const { tokens } = await appSignsIn(page, request, callback, baseURL!, "Sam's phone");
    const bearer = { authorization: `Bearer ${tokens.access_token}` };
    // A person who is not the owner sees nothing of it.
    await page.goto("/healthz");
    await signIn("dana", SECURITY);
    await expect(page.getByText("No device is signed in.")).toBeVisible();
    await expect(page.getByText("Sam's phone")).toHaveCount(0);
    // The administrator sees it, with the person it belongs to; a name is the owner's.
    await page.goto("/healthz");
    await signIn("root", SECURITY);
    const devices = page.getByRole("table", { name: "Devices" });
    await expect(devices.getByRole("columnheader", { name: "Person" })).toBeVisible();
    const row = devices.getByRole("row", { name: /Sam's phone/ });
    await expect(row).toContainText("sam");
    await expect(row.getByRole("button", { name: "Rename Sam's phone" })).toHaveCount(0);
    // Who is signed in is what the app holds, not what the browser's storage says: with the
    // stored name gone (a private window, blocked site data) and the table opened again, the
    // device is still not root's to rename, and the Person column is still there.
    await page.evaluate(() => sessionStorage.removeItem("obop-user"));
    await page.evaluate(() => {
      location.hash = "#/customize/theme?tab=theme";
    });
    await expect(page.getByRole("table", { name: "Devices" })).toHaveCount(0);
    await page.evaluate((h) => {
      location.hash = h;
    }, `#${SECURITY}`);
    const again = page.getByRole("table", { name: "Devices" });
    await expect(again.getByRole("columnheader", { name: "Person" })).toBeVisible();
    await expect(
      again.getByRole("row", { name: /Sam's phone/ }).getByRole("button", { name: /^Rename/ }),
    ).toHaveCount(0);
    // But the device can be ended, and the app is out at once.
    await again.getByRole("button", { name: "Revoke Sam's phone" }).click();
    await expect(page.getByText("Sam's phone is signed out.")).toBeVisible();
    await expect(again.getByRole("row", { name: /Sam's phone/ })).toContainText("Revoked");
    expect((await request.post("/api/party/list", { headers: bearer, data: {} })).status()).toBe(
      401,
    );
  });

  test("a server that cannot answer is said so, and the table does not disappear", async ({
    page,
    request,
    callback,
    signIn,
    baseURL,
    allowHttpErrors,
  }) => {
    test.setTimeout(120_000);
    allowHttpErrors(503); // the two calls the test makes refuse on purpose
    await appSignsIn(page, request, callback, baseURL!, "Busy phone");
    await page.goto("/healthz");
    await signIn("sam", SECURITY);
    const devices = page.getByRole("table", { name: "Devices" });
    await expect(devices.getByRole("row", { name: /Busy phone/ })).toHaveCount(1);

    // The revocation is refused by a busy server: the person is told to ask again, the row stays.
    await page.route(/\/api\/device\/revoke/, (route) =>
      route.fulfill({
        status: 503,
        headers: { "retry-after": "1", "content-type": "application/json" },
        body: JSON.stringify({ error: "storage_busy" }),
      }),
    );
    await devices.getByRole("button", { name: "Revoke Busy phone" }).click();
    await expect(
      page.getByText("The server is busy. Ask again in a moment; it finishes where it stopped."),
    ).toBeVisible();
    await expect(devices.getByRole("row", { name: /Busy phone/ })).toContainText("Signed in");
    await page.unroute(/\/api\/device\/revoke/);

    // The list is refused by a busy server: a notice with a way to try again, not an empty page.
    await page.route(/\/api\/device\/search/, (route) =>
      route.fulfill({
        status: 503,
        headers: { "retry-after": "1", "content-type": "application/json" },
        body: JSON.stringify({ error: "storage_busy" }),
      }),
    );
    await page.reload();
    await expect(
      page.getByText("The devices could not be loaded because the server is busy."),
    ).toBeVisible();
    await page.unroute(/\/api\/device\/search/);
    await page.getByRole("button", { name: "Try again" }).click();
    await expect(
      page.getByRole("table", { name: "Devices" }).getByRole("row", { name: /Busy phone/ }),
    ).toHaveCount(1);
  });
});
