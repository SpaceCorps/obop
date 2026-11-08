import { createHash, randomBytes } from "node:crypto";
import type { APIRequestContext, Page, TestInfo } from "@playwright/test";
import { brand, brandUrl, users } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// The OAuth pages of an instance with a brand (docs/spec/branding.md, slice S1c). The rendering is
// held by the Rust tests; this spec is the proof that needs a real browser, three claims the spec
// made without trying them (section 14, "Honest limits"):
//
//  1. an inline SVG with presentation attributes and a fixed size passes a policy whose style-src
//     is a hash and whose default-src is 'none', and so does the page's own style sheet;
//  2. the policy with `img-src 'self'` lets the page load the tab icon, the logo route;
//  3. the logo sits on the line of the text, with no rule added to the style sheet.
//
// A fourth instance is started with the brand set at the terminal (support/serve.ts, "brand"); the
// client is registered with the real registration, and the browser stops at the sign-in and consent
// pages of the real flow. Nothing is clicked that would end the flow.
const REDIRECT = "https://claude.ai/api/mcp/auth_callback";
const CLIENT = "Acme Notes";

function challenge() {
  return createHash("sha256").update(randomBytes(32).toString("base64url")).digest("base64url");
}

async function register(request: APIRequestContext, name: string): Promise<string> {
  const res = await request.post(`${brandUrl()}/oauth/register`, {
    data: { client_name: name, redirect_uris: [REDIRECT], token_endpoint_auth_method: "none" },
  });
  expect(res.status()).toBe(201);
  return (await res.json()).client_id as string;
}

function authorizeUrl(clientId: string) {
  return `${brandUrl()}/oauth/authorize?${new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: REDIRECT,
    state: "state-1",
    code_challenge: challenge(),
    code_challenge_method: "S256",
    resource: `${brandUrl()}/mcp`,
  })}`;
}

/** Records every `securitypolicyviolation` the page reports, from the first byte of the document. */
async function watchPolicy(page: Page) {
  await page.addInitScript(() => {
    const seen: string[] = [];
    (window as any).__violations = seen;
    document.addEventListener("securitypolicyviolation", (e) =>
      seen.push(`${e.violatedDirective}: ${e.blockedURI}`),
    );
  });
}

const violations = (page: Page) => page.evaluate(() => (window as any).__violations as string[]);

async function signIn(page: Page) {
  await page.getByLabel("Username").fill(users.sam.name);
  await page.getByLabel("Password").fill(users.sam.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("heading", { name: `Connect ${CLIENT}?` })).toBeVisible();
}

/** Where the logo and the text of the line above the card sit, in page pixels. */
async function lineBoxes(page: Page) {
  return page.evaluate(() => {
    const line = document.querySelector(".brand")!;
    const svg = line.querySelector("svg")!.getBoundingClientRect();
    const range = document.createRange();
    range.selectNodeContents(line.lastChild!);
    const text = range.getBoundingClientRect();
    return {
      svg: { top: svg.top, bottom: svg.bottom, width: svg.width, height: svg.height },
      text: { top: text.top, bottom: text.bottom, height: text.height },
    };
  });
}

test.describe("the OAuth pages of an instance with a brand", () => {
  let client: string;
  test.beforeAll(async ({ playwright }) => {
    const request = await playwright.request.newContext();
    client = await register(request, CLIENT);
    await request.dispose();
  });

  test("the sign-in page shows the logo, the name and the message, and the browser reports no violation", async ({
    page,
  }, info: TestInfo) => {
    await watchPolicy(page);
    const res = await page.goto(authorizeUrl(client));
    expect(res!.status()).toBe(200);

    // The policy is the one the page always had with `img-src 'self'` added, and the style sheet is
    // still allowed by its hash alone: no 'unsafe-inline', no new source.
    const csp = res!.headers()["content-security-policy"]!;
    expect(csp).toMatch(/^default-src 'none'; style-src 'sha256-[A-Za-z0-9+/=]+'; /);
    expect(csp).toMatch(/; img-src 'self'$/);
    expect(csp).not.toMatch(/unsafe-|nonce/);

    // The line above the card: the logo, then the name. The logo is the inline SVG the server wrote,
    // 28 by 28, drawn by the browser under that policy.
    const line = page.locator(".brand");
    await expect(line).toHaveText(new RegExp(`^\\s*${brand.name}\\s*$`));
    const logo = line.locator("svg");
    await expect(logo).toBeVisible();
    const box = (await logo.boundingBox())!;
    expect([box.width, box.height]).toEqual([28, 28]);
    // It is decoration: a screen reader reads the name once.
    await expect(line.locator("[aria-hidden=true] svg")).toHaveCount(1);
    await expect(page.getByRole("img")).toHaveCount(0);

    // The sentences name no product, the message sits under the lead, the client is named.
    await expect(page.getByText(brand.message)).toBeVisible();
    await expect(page.getByText(`${CLIENT} wants to connect to this instance.`)).toBeVisible();
    await expect(page.locator("body")).not.toContainText(/obop/i, { useInnerText: true });
    expect(await page.title()).not.toMatch(/obop/i);

    // The tab icon: a link to the logo route with its version, and an image of that address loads
    // under the page's own policy (without `img-src` the policy would refuse it).
    const icon = page.locator('link[rel="icon"]');
    await expect(icon).toHaveCount(1);
    const href = (await icon.getAttribute("href"))!;
    expect(href).toMatch(/^\/api\/brand\/logo\.svg\?v=[0-9a-f]{16}$/);
    const loaded = await page.evaluate(
      (src) =>
        new Promise<boolean>((done) => {
          const img = new Image();
          img.onload = () => done(true);
          img.onerror = () => done(false);
          img.src = src;
        }),
      href,
    );
    expect(loaded, "an image of the icon address loads under the page's policy").toBe(true);
    const served = await page.request.get(`${brandUrl()}${href}`);
    expect(served.status()).toBe(200);
    expect(served.headers()["content-type"]).toBe("image/svg+xml");
    expect(await served.text()).toBe(brand.artwork);

    // The logo sits on the line of the text: the text box overlaps the logo box by most of its
    // height (no vertical-align rule was added to the style sheet; if this fails the fix is a
    // second constant style sheet with its own hash, which the spec says is not built here).
    const { svg, text } = await lineBoxes(page);
    const overlap = Math.min(svg.bottom, text.bottom) - Math.max(svg.top, text.top);
    await info.attach("line-boxes", {
      body: JSON.stringify({ svg, text, overlap }, null, 2),
      contentType: "application/json",
    });
    await info.attach("sign-in-brand-line", {
      body: await line.screenshot(),
      contentType: "image/png",
    });
    expect(overlap / text.height).toBeGreaterThan(0.5);

    expect(await violations(page)).toEqual([]);
  });

  test("the consent page keeps the claim and the host line under the brand", async ({ page }) => {
    await watchPolicy(page);
    await page.goto(authorizeUrl(client));
    await signIn(page);
    const box = page.locator(".notice");
    await expect(box).toContainText("Who is asking");
    // The claim, with the application named twice and no product; the host the result goes to.
    await expect(box).toContainText(`This instance cannot verify who runs ${CLIENT}.`);
    await expect(box.locator("strong", { hasText: CLIENT })).toHaveCount(2);
    await expect(box.locator("strong", { hasText: "claude.ai" })).toHaveCount(1);
    await expect(box).not.toContainText("This name is the same as the name of this instance");
    await expect(page.getByRole("button", { name: "Allow", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Deny", exact: true })).toBeVisible();
    await expect(page.locator(".brand svg")).toBeVisible();
    await expect(page.locator("body")).not.toContainText(/obop/i, { useInnerText: true });
    expect(await violations(page)).toEqual([]);
  });

  test("a client that registers under the name of the instance is told so", async ({
    page,
    request,
  }) => {
    const hostile = await request.post(`${brandUrl()}/oauth/register`, {
      data: {
        client_name: `${brand.name.slice(0, 2)}‍${brand.name.slice(2).toLowerCase()}`,
        redirect_uris: [REDIRECT],
        token_endpoint_auth_method: "none",
      },
    });
    expect(hostile.status()).toBe(201);
    await watchPolicy(page);
    await page.goto(authorizeUrl((await hostile.json()).client_id));
    await page.getByLabel("Username").fill(users.sam.name);
    await page.getByLabel("Password").fill(users.sam.password);
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await expect(page.locator(".notice")).toContainText(
      "This name is the same as the name of this instance. That does not mean that the application belongs to this instance.",
    );
    // The line above the card is the only place the instance's name stands apart from the client's.
    await expect(page.locator(".brand")).toHaveText(new RegExp(`^\\s*${brand.name}\\s*$`));
    expect(await violations(page)).toEqual([]);
  });

  test.describe("on a phone", () => {
    test.use({ viewport: { width: 375, height: 812 } });

    test("the line and the card fit the width, and the logo is there", async ({ page }) => {
      await watchPolicy(page);
      await page.goto(authorizeUrl(client));
      await expect(page.locator(".brand svg")).toBeVisible();
      await expect(page.locator(".brand")).toHaveText(new RegExp(`^\\s*${brand.name}\\s*$`));
      const sideways = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(sideways).toBeLessThanOrEqual(0);
      expect(await violations(page)).toEqual([]);
    });
  });

  test("an error page of the flow shows the brand too", async ({ page, allowHttpErrors }) => {
    allowHttpErrors(400);
    await watchPolicy(page);
    const res = await page.goto(
      `${brandUrl()}/oauth/authorize?client_id=oc_not_registered&redirect_uri=${encodeURIComponent(REDIRECT)}&response_type=code`,
    );
    expect(res!.status()).toBe(400);
    await expect(page.locator(".brand svg")).toBeVisible();
    await expect(page.locator(".brand")).toHaveText(new RegExp(`^\\s*${brand.name}\\s*$`));
    await expect(page.locator("body")).not.toContainText(/obop/i, { useInnerText: true });
    expect(await violations(page)).toEqual([]);
  });
});
