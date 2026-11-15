import type { Page } from "@playwright/test";
import { users, type UserName } from "../support/env.ts";
import { HOST_USER_AGENT, expect, test } from "../support/host.ts";
import { base32Decode, hotp, STEP } from "../support/totp.ts";

// The back office as the page inside the phone host app (web/apps/host-app). The host is a
// stand-in (support/host.ts): the context's user agent ends in the host's token, the page finds
// the WebView's transport, and its messages are answered in Node by the host's side of
// `@obop/host-bridge`, the code the app runs. So what is under test is the page's side
// (web/apps/backoffice/src/host), the protocol and the host's rules; no phone and no WebView.

const SECURITY = "/#/customize/security?tab=security";
const DOWNLOADS = "Downloads are not available in the app yet.";
const CODES = "Copy the codes or write them down.";

const html = (page: Page) => page.locator("html");
const passkeyButton = (page: Page) => page.getByRole("button", { name: "Sign in with a passkey" });

async function signInForm(page: Page, user: UserName) {
  await page.goto("/");
  await page.getByLabel("Username").fill(users[user].name);
  await page.getByLabel("Password").fill(users[user].password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.locator(".topbar")).toBeVisible();
}

/** The names of the entries of the open account menu, in order. */
async function accountMenu(page: Page): Promise<string[]> {
  await page.getByRole("button", { name: "Account" }).click();
  await expect(page.getByRole("menuitem", { name: "Sign out" })).toBeVisible();
  return (await page.getByRole("menuitem").allTextContents()).map((text) => text.trim());
}

/** The colour of the shell's ground as the screen paints it, `#rrggbb`: read through a canvas. */
function ground(page: Page): Promise<string> {
  return page.evaluate(() => {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    const context = canvas.getContext("2d")!;
    context.fillStyle = getComputedStyle(document.querySelector(".canvas")!).backgroundColor;
    context.fillRect(0, 0, 1, 1);
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
    return `#${[r, g, b].map((part) => part!.toString(16).padStart(2, "0")).join("")}`;
  });
}

/** Counts the files the browser was asked to save. */
function downloads(page: Page): () => number {
  let count = 0;
  page.on("download", () => {
    count += 1;
  });
  return () => count;
}

test.describe("a browser that is not the host app", () => {
  // The transport of a WebView is there, the host's token in the user agent is not: both facts
  // are needed, so this is a browser, and a page that spoke to the transport anyway is caught.
  test.use({ hostUserAgent: false });

  test("the back office is as it was: the passkey button, no This app, nothing said to a host", async ({
    page,
    host,
  }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
    await expect(passkeyButton(page)).toBeVisible();
    expect(await page.evaluate(() => navigator.userAgent)).not.toContain("ObopHost");
    const marks = await page.evaluate(() => ({
      transport: typeof (window as unknown as { ReactNativeWebView?: object }).ReactNativeWebView,
      receiver: "__obopHost" in window,
      host: document.documentElement.getAttribute("data-host"),
    }));
    expect(marks).toEqual({ transport: "object", receiver: false, host: null });

    await signInForm(page, "sam");
    const entries = await accountMenu(page);
    expect(entries).not.toContain("This app");
    expect(entries.at(-1)).toBe("Sign out");
    expect(host.posted).toEqual([]);
    expect(host.calls).toEqual([]);
  });
});

test.describe("inside the host app", () => {
  test("the page asks host.info exactly once, and the sign-in page offers no passkey", async ({
    page,
    host,
  }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
    const userAgent = await page.evaluate(() => navigator.userAgent);
    expect(userAgent.endsWith(` ${HOST_USER_AGENT}`), userAgent).toBe(true);
    await expect(html(page)).toHaveAttribute("data-host", "1");
    await expect(html(page)).toHaveAttribute("data-host-info", "ok");
    // This browser knows passkeys, so it is the host's answer that takes the button away.
    expect(await page.evaluate(() => typeof window.PublicKeyCredential)).toBe("function");
    await expect(passkeyButton(page)).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Sign in with Microsoft" })).toHaveCount(0);
    expect(host.paramsOf("host.info")).toEqual([{}]);
    // The receiver is the page's own and cannot be replaced by a script.
    const receiver = await page.evaluate(() => {
      const before = (window as unknown as { __obopHost: unknown }).__obopHost;
      try {
        (window as unknown as { __obopHost: unknown }).__obopHost = {};
      } catch {
        // a module script is strict: the write throws
      }
      return (window as unknown as { __obopHost: unknown }).__obopHost === before;
    });
    expect(receiver).toBe(true);
    expect(host.calls[0]).toEqual({ command: "host.info", params: {} });
  });

  test("the page tells the host its ground colour, and again when the person chooses Dark", async ({
    page,
    host,
    signIn,
  }) => {
    await signIn("sam", "/customize/theme?tab=theme");
    await expect(page.getByRole("radio", { name: "Dark" })).toBeVisible();
    const light = await ground(page);
    await expect
      .poll(() => host.paramsOf("host.theme").at(-1))
      .toEqual({
        scheme: "light",
        background: light,
      });

    // Chosen and not saved: the theme applies at once and is gone with the page.
    await page.getByRole("radio", { name: "Dark" }).click();
    await expect(html(page)).toHaveAttribute("data-theme", "dark");
    const dark = await ground(page);
    expect(dark).not.toBe(light);
    await expect
      .poll(() => host.paramsOf("host.theme").at(-1))
      .toEqual({
        scheme: "dark",
        background: dark,
      });
    // One report per ground, not one per attribute the theme writes.
    expect(
      host.paramsOf("host.theme").filter((p) => (p as { scheme: string }).scheme === "dark"),
    ).toHaveLength(1);
  });

  test("Account, This app asks the host for its own screen", async ({ page, host, signIn }) => {
    await signIn("sam");
    const entries = await accountMenu(page);
    // Directly above Sign out.
    expect(entries.slice(-2)).toEqual(["This app", "Sign out"]);
    await page.getByRole("menuitem", { name: "This app" }).click();
    await expect.poll(() => host.paramsOf("host.settings")).toEqual([{}]);
  });

  test("Export on a list saves no file and says that downloads are not available", async ({
    page,
    host,
    api,
    signIn,
    scratchModule,
  }) => {
    const model = await scratchModule({
      title: "Host things",
      fields: [{ name: "title", required: true }],
    });
    await api.create("root", model, { title: "One thing" });
    const saved = downloads(page);
    await signIn("root", `/${model}/${model}?view=list`);
    await expect(page.getByRole("grid")).toBeVisible();
    await expect(html(page)).toHaveAttribute("data-host-info", "ok");
    await page.getByRole("checkbox", { name: "Select all records" }).check();
    await page.getByRole("button", { name: "Export selected" }).click();
    await expect(page.getByText(DOWNLOADS, { exact: true })).toBeVisible();
    // A download would have been announced by now.
    await page.waitForTimeout(500);
    expect(saved()).toBe(0);
    expect(host.paramsOf("host.info")).toHaveLength(1);
  });

  test("the recovery codes are not downloaded, and the person is told to copy them", async ({
    page,
    api,
  }) => {
    test.setTimeout(60_000);
    const { password } = users.tess;
    const saved = downloads(page);
    // Signed in through the form: the session that sets two-factor up is the one that ends it.
    await signInForm(page, "tess");
    await page.goto(SECURITY);
    await expect(page.getByRole("heading", { name: /^Two-factor authentication/ })).toBeVisible();
    await page.getByRole("button", { name: "Set up two-factor" }).click();
    const secret = ((await page.getByLabel("Setup key").textContent()) ?? "").replace(/\s/g, "");
    expect(secret).toMatch(/^[A-Z2-7]{32}$/);
    const step = Math.floor(Date.now() / 1000 / STEP);
    await page.getByLabel("6-digit code").fill(hotp(base32Decode(secret), step));
    await page.getByRole("button", { name: "Turn on" }).click();
    try {
      await expect(page.getByRole("heading", { name: "Save your recovery codes" })).toBeVisible();
      await page.getByRole("button", { name: "Download" }).click();
      await expect(page.getByText(`${DOWNLOADS} ${CODES}`, { exact: true })).toBeVisible();
      await page.waitForTimeout(500);
      expect(saved()).toBe(0);
      // The codes are still on the screen to be copied, and the person can go on.
      await expect(page.getByRole("list", { name: "Recovery codes" }).locator("code")).toHaveCount(
        10,
      );
      await page.getByLabel("I have saved these codes").check();
      await page.getByRole("button", { name: "Done" }).click();
      await expect(page.getByText("10 recovery codes left.")).toBeVisible();
    } finally {
      // tess belongs to tests/mfa.spec.ts, which starts with two-factor off: turn it off again,
      // with a code of a step that the set-up has not used.
      const token = (await page.evaluate(() => sessionStorage.getItem("obop-token"))) ?? "";
      if ((await api.get("/api/auth/mfa", token)).json?.enabled === true) {
        const now = Math.floor(Date.now() / 1000 / STEP);
        const off = await api.post(
          "/api/auth/mfa/disable",
          { password, code: hotp(base32Decode(secret), Math.max(now, step + 1)) },
          token,
        );
        expect(off.status, JSON.stringify(off.json)).toBe(200);
      }
    }
  });
});

test.describe("a host that reports another sender", () => {
  // What the WebView reports when the message came from a frame of another origin. The host's
  // first rule gives such a message no answer.
  test.use({ hostSender: "https://elsewhere.example/" });

  test("the page's request ends in failed, no handler runs, and the page offers nothing", async ({
    page,
    host,
  }) => {
    await page.goto("/");
    await expect(html(page)).toHaveAttribute("data-host", "1");
    // The page waits five seconds for an answer.
    await expect(html(page)).toHaveAttribute("data-host-info", "failed", { timeout: 10_000 });
    expect(host.posted.map((json) => (JSON.parse(json) as { command: string }).command)).toEqual([
      "host.info",
    ]);
    expect(host.calls).toEqual([]);
    await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
    await expect(passkeyButton(page)).toHaveCount(0);
  });
});
