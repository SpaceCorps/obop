import { readFile } from "node:fs/promises";
import type { APIRequestContext, Page } from "@playwright/test";
import { brand, brandUrl, platformUrl, shopOwner, shopUrl, users } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";
import { base32Decode, hotp, STEP } from "../support/totp.ts";

// The back office of an instance with a brand (docs/spec/branding.md, slice S2): the sign-in
// screens, the screens that come before the app (a new password, a second factor), the tab, the top
// bar, the recovery-codes file, the pages that document the product, and the Identity tab. Run at
// the root of an instance (the `brand` server, branded at the terminal before it starts) and under
// `/admin/` (the shop server, branded here through the command and cleared afterwards), at 1280 and
// 375 pixels. The rendering is held by the unit tests; this is what needs a real browser.

type Who = { name: string; password: string };
type Layout = {
  name: string;
  /** The server, without a trailing slash. */
  base: () => string;
  /** Where the back office is. */
  path: string;
  owner: Who;
  /** Whether this spec brands the server (and clears it again) instead of finding it branded. */
  brands: boolean;
};

const LAYOUTS: Layout[] = [
  { name: "at the root", base: brandUrl, path: "/", owner: users.root, brands: false },
  { name: "under /admin/", base: shopUrl, path: "/admin/", owner: shopOwner, brands: true },
];
const SIZES = [
  { name: "1280 px", width: 1280, height: 800 },
  { name: "375 px", width: 375, height: 800 },
] as const;
const PASSWORD = "e2e brand password 1!";
const stepNow = () => Math.floor(Date.now() / 1000 / STEP);

async function call(
  request: APIRequestContext,
  base: string,
  token: string | undefined,
  id: string,
  body: object = {},
) {
  const [model, command] = id.split(".");
  const res = await request.post(`${base}/api/${model}/${command}`, {
    data: body,
    headers: token ? { authorization: `Bearer ${token}` } : {},
  });
  return { status: res.status(), json: (await res.json().catch(() => null)) as any };
}

async function tokenOf(request: APIRequestContext, base: string, who: Who): Promise<string> {
  const res = await request.post(`${base}/api/auth/login`, {
    data: { username: who.name, password: who.password },
  });
  expect(res.status(), `login as ${who.name}`).toBe(200);
  return (await res.json()).token as string;
}

/** A person with the groups, who has to choose a password first when `mustChange`. */
async function addPerson(
  request: APIRequestContext,
  base: string,
  admin: string,
  name: string,
  mustChange: boolean,
  groups = "base.user",
): Promise<Who> {
  const res = await request.post(`${base}/api/auth/users`, {
    data: { username: name, password: PASSWORD, groups, must_change_password: mustChange },
    headers: { authorization: `Bearer ${admin}` },
  });
  expect(res.status(), `create ${name}: ${await res.text()}`).toBe(200);
  return { name, password: PASSWORD };
}

async function formSignIn(page: Page, url: string, who: Who) {
  await page.goto(url);
  await page.getByLabel("Username").fill(who.name);
  await page.getByLabel("Password").fill(who.password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
}

/** Open the back office already signed in as `who`, with the token the login gave. */
async function openAs(page: Page, url: string, token: string, name: string) {
  await page.addInitScript(
    ([t, u]) => {
      sessionStorage.setItem("obop-token", t!);
      sessionStorage.setItem("obop-user", u!);
    },
    [token, name],
  );
  await page.goto(url);
  await expect(page.locator(".topbar")).toBeVisible();
}

/**
 * Nothing on the screen, in any name a screen reader reads, in the tab title or in a tooltip says
 * obop. One operator hint is the exception: an instance reached by an IP address (as these servers
 * are) says that passkeys need a host name and names the setting `OBOP_PUBLIC_URL`; a real instance
 * has a host name and never shows it. The text is the server's, in Rust, and outside this slice.
 */
async function expectNoProductName(page: Page) {
  const seen = await page.evaluate(() => {
    const out = [`title: ${document.title}`, `text: ${document.body.innerText}`];
    for (const el of document.querySelectorAll("[aria-label],[alt],[title],[placeholder]"))
      for (const a of ["aria-label", "alt", "title", "placeholder"]) {
        const v = el.getAttribute(a);
        if (v) out.push(`${a}: ${v}`);
      }
    return out;
  });
  const hint = / ?\(set OBOP_PUBLIC_URL\)/g;
  for (const piece of seen) expect(piece.replace(hint, "")).not.toMatch(/obop/i);
  expect((await page.locator("body").ariaSnapshot()).replace(hint, "")).not.toMatch(/obop/i);
}

/** The logo shown on the screen is the instance's: one image, loaded, from the logo route. */
async function expectBrandLogo(page: Page) {
  const logo = page.locator("h1 img:visible, .gate-head img:visible, .topbar img:visible");
  await expect(logo).toHaveCount(1);
  await expect(logo).toHaveAttribute("src", /^\/api\/brand\/logo\.svg\?v=[0-9a-f]{16}$/);
  expect(await logo.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(
    true,
  );
}

/** The tab: the name as the title and the logo route as the one icon, which the server serves. */
async function expectBrandTab(page: Page) {
  await expect(page).toHaveTitle(brand.name);
  const icons = page.locator('link[rel~="icon" i]');
  await expect(icons).toHaveCount(1);
  const href = (await icons.getAttribute("href"))!;
  expect(href).toMatch(/^\/api\/brand\/logo\.svg\?v=[0-9a-f]{16}$/);
  const served = await page.evaluate(async (h) => {
    const r = await fetch(h);
    return [r.status, r.headers.get("content-type")];
  }, href);
  expect(served).toEqual([200, "image/svg+xml"]);
}

async function enrol(page: Page): Promise<string[]> {
  await page.getByRole("button", { name: "Set up two-factor" }).click();
  await expect(page.getByRole("img", { name: "QR code for the authenticator app" })).toBeVisible();
  const secret = ((await page.getByLabel("Setup key").textContent()) ?? "").replace(/\s/g, "");
  await page.getByLabel("6-digit code").fill(hotp(base32Decode(secret), stepNow()));
  await page.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByRole("heading", { name: "Save your recovery codes" })).toBeVisible();
  return page.getByRole("list", { name: "Recovery codes" }).locator("code").allTextContents();
}

async function downloadName(page: Page, codes: string[]): Promise<string> {
  const [file] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Download" }).click(),
  ]);
  const text = await readFile((await file.path())!, "utf8");
  for (const c of codes) expect(text).toContain(c);
  return file.suggestedFilename();
}

for (const layout of LAYOUTS) {
  test.describe(`the back office of an instance with a brand, ${layout.name}`, () => {
    test.describe.configure({ mode: "serial" });
    const url = () => `${layout.base()}${layout.path}`;
    let admin = "";

    test.beforeAll(async ({ playwright }) => {
      const request = await playwright.request.newContext();
      try {
        admin = await tokenOf(request, layout.base(), layout.owner);
        if (layout.brands) {
          const r = await call(request, layout.base(), admin, "instance_brand.configure", {
            name: brand.name,
            message_en: brand.message,
            logo_svg: brand.artwork,
          });
          expect(r.status, JSON.stringify(r.json)).toBe(200);
        }
        const r = await call(request, layout.base(), admin, "instance_brand.configure", {
          hide_product_docs: false,
        });
        expect(r.status, JSON.stringify(r.json)).toBe(200);
      } finally {
        await request.dispose();
      }
    });

    test.afterAll(async ({ playwright }) => {
      if (!layout.brands) return;
      const request = await playwright.request.newContext();
      try {
        await call(request, layout.base(), admin, "instance_brand.clear");
      } finally {
        await request.dispose();
      }
    });

    for (const size of SIZES)
      test(`the sign-in screen shows the brand, ${size.name}`, async ({ page }) => {
        await page.setViewportSize(size);
        await page.goto(url());
        await expect(page.getByRole("heading", { name: "Sign in", level: 2 })).toBeVisible();
        await expectBrandTab(page);
        await expectBrandLogo(page);
        await expect(page.locator("h1:visible")).toHaveText(brand.name);
        // The message sits under the hint line, and the product's own copy is gone.
        await expect(page.getByText(brand.message, { exact: true })).toBeVisible();
        await expect(page.getByText("Sign in to your instance.")).toBeVisible();
        await expect(page.getByText("One business, one platform.")).toHaveCount(0);
        await expect(page.getByText("Personal data is masked unless you may see it")).toHaveCount(
          0,
        );
        await expectNoProductName(page);
        expect(await layoutProblems(page)).toEqual([]);
      });

    test("the sign-in screen takes the accent that the theme stores, before anyone has signed in", async ({
      page,
      request,
    }) => {
      const before = (await call(request, layout.base(), admin, "setting.get", { id: "instance" }))
        .json.value.value as string;
      const stored = JSON.parse(before);
      const withAccent = JSON.stringify({
        ...stored,
        theme: { ...stored.theme, accent: "#c2410c" },
      });
      const accentOf = () =>
        page.evaluate(() =>
          getComputedStyle(document.documentElement).getPropertyValue("--accent").trim(),
        );
      try {
        // Without a stored accent the sign-in screen has the stock one.
        await page.goto(url());
        await expect(page.getByRole("heading", { name: "Sign in", level: 2 })).toBeVisible();
        const stock = await accentOf();
        expect(stock).not.toBe("#c2410c");
        await call(request, layout.base(), admin, "setting.update", {
          id: "instance",
          value: withAccent,
        });
        // The shell is read through the reader, which keeps its answer for two seconds.
        await expect(async () => {
          await page.goto(url());
          await expect(page.getByRole("heading", { name: "Sign in", level: 2 })).toBeVisible();
          expect(await accentOf()).toBe("#c2410c");
        }).toPass({ timeout: 15_000 });
      } finally {
        await call(request, layout.base(), admin, "setting.update", {
          id: "instance",
          value: before,
        });
      }
    });

    test("after sign-in the tab still carries the name, and so does the top bar", async ({
      page,
      request,
    }) => {
      const sam = await addPerson(request, layout.base(), admin, `viewer${Date.now()}`, false);
      await formSignIn(page, url(), sam);
      await expect(page.locator(".topbar")).toBeVisible();
      await expectBrandTab(page);
      await expectBrandLogo(page);
      await expect(page.locator(".topbar")).toContainText(brand.name);
      await expectNoProductName(page);
    });

    test("a person who must choose a password first meets the brand there too", async ({
      page,
      request,
      allowHttpErrors,
    }) => {
      allowHttpErrors(403);
      const who = await addPerson(request, layout.base(), admin, `change${Date.now()}`, true);
      await formSignIn(page, url(), who);
      await expect(page.getByText("Choose a new password to continue")).toBeVisible();
      await expect(page.locator(".topbar")).toHaveCount(0);
      for (const size of SIZES) {
        await page.setViewportSize(size);
        await expectBrandTab(page);
        await expectBrandLogo(page);
        await expect(page.locator(".gate-head")).toContainText(brand.name);
        await expectNoProductName(page);
        expect(await layoutProblems(page), size.name).toEqual([]);
      }
    });

    test("the second-factor set-up screen has the brand, and the recovery file is named by it", async ({
      page,
      request,
      allowHttpErrors,
    }) => {
      test.setTimeout(60_000);
      allowHttpErrors(403);
      const who = await addPerson(request, layout.base(), admin, `second${Date.now()}`, false);
      const before = (await call(request, layout.base(), admin, "setting.get", { id: "instance" }))
        .json.value.value as string;
      const withPolicy = JSON.stringify({
        ...JSON.parse(before),
        security: { require_mfa_for_all: true },
      });
      await call(request, layout.base(), admin, "setting.update", {
        id: "instance",
        value: withPolicy,
      });
      try {
        await formSignIn(page, url(), who);
        await expect(page.getByText("Set up two-factor authentication to continue")).toBeVisible();
        await expect(page.locator(".topbar")).toHaveCount(0);
        for (const size of SIZES) {
          await page.setViewportSize(size);
          await expectBrandTab(page);
          await expectBrandLogo(page);
          await expect(page.locator(".gate-head")).toContainText(brand.name);
          await expectNoProductName(page);
          expect(await layoutProblems(page), size.name).toEqual([]);
        }
        await page.setViewportSize(SIZES[0]);
        const codes = await enrol(page);
        expect(codes).toHaveLength(10);
        // The set-up and its recovery codes say nothing of the product either.
        await expectNoProductName(page);
        expect(await downloadName(page, codes)).toBe("mohv-recovery-codes.txt");
      } finally {
        await call(request, layout.base(), admin, "setting.update", {
          id: "instance",
          value: before,
        });
      }
    });
  });
}

test.describe("the sign-in screen in another language", () => {
  test.use({ language: null, locale: "sv-SE" });
  for (const size of SIZES)
    test(`a Swedish screen with only an English message shows the English one, ${size.name}`, async ({
      page,
    }) => {
      await page.setViewportSize(size);
      await page.goto(`${brandUrl()}/`);
      // Swedish: the page speaks it; the message was written in English only.
      await expect(page.getByRole("heading", { name: "Logga in", level: 2 })).toBeVisible();
      await expect(page.getByText(brand.message, { exact: true })).toBeVisible();
      // It is English text on a Swedish page: a screen reader is told so.
      await expect(page.getByText(brand.message, { exact: true })).toHaveAttribute("lang", "en");
      await expect(page).toHaveTitle(brand.name);
      await expectNoProductName(page);
    });
});

test.describe("the pages that document the product", () => {
  test.describe.configure({ mode: "serial" });
  const base = brandUrl();
  let admin = "";
  let sam = "";
  let samName = "";

  test.beforeAll(async ({ playwright }) => {
    const request = await playwright.request.newContext();
    try {
      admin = await tokenOf(request, base, users.root);
      sam = await tokenOf(request, base, users.sam);
      samName = users.sam.name;
    } finally {
      await request.dispose();
    }
  });
  test.afterAll(async ({ playwright }) => {
    const request = await playwright.request.newContext();
    try {
      await call(request, base, admin, "instance_brand.configure", { hide_product_docs: false });
    } finally {
      await request.dispose();
    }
  });

  const hide = (request: APIRequestContext, on: boolean) =>
    call(request, base, admin, "instance_brand.configure", { hide_product_docs: on });

  test("an administrator sees them until the switch is on, and then in none of the seven places", async ({
    page,
    request,
    allowHttpErrors,
  }) => {
    test.setTimeout(60_000);
    allowHttpErrors(404);
    const tab = (name: string) => page.getByRole("tab", { name, exact: true });
    const menu = async () => {
      await page.locator("button.avatar-btn").click();
      return page.getByRole("menuitem");
    };
    const palette = async (query: string) => {
      await page.keyboard.press("Control+k");
      await page.getByRole("dialog", { name: "Command palette" }).locator("input").fill(query);
    };

    // With the switch off, as ever.
    expect((await hide(request, false)).status).toBe(200);
    await openAs(page, `${base}/#/customize/theme?tab=theme`, admin, users.root.name);
    await expect(tab("Brand book")).toBeVisible();
    await expect(tab("Guide")).toBeVisible();
    await expect(tab("Identity")).toBeVisible();
    await expect(page.getByText("Looking for the rules?")).toBeVisible();
    await expect(page.getByRole("button", { name: "Open the Brand book" })).toBeVisible();
    await expect(await menu()).toContainText(["Theme", "Security", "Brand book"]);
    await page.keyboard.press("Escape");
    await palette("custom module");
    await expect(page.getByRole("option", { name: /How to write a custom module/ })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.goto(`${base}/#/customize/modules?tab=modules`);
    await expect(page.getByRole("link", { name: "How to write a custom module?" })).toBeVisible();
    await page.goto(`${base}/#/customize/build?tab=build`);
    await expect(page.getByRole("link", { name: "How to write a custom module?" })).toBeVisible();

    // Switch on, and the page that was open reads it after a reload.
    expect((await hide(request, true)).status).toBe(200);
    await page.goto(`${base}/#/customize/theme?tab=theme`);
    await page.reload();
    await expect(page.locator(".topbar")).toBeVisible();
    await expect(tab("Theme")).toBeVisible();
    await expect(tab("Brand book")).toHaveCount(0);
    await expect(tab("Guide")).toHaveCount(0);
    await expect(tab("Identity")).toBeVisible(); // the tab of the switch itself stays
    await expect(page.getByText("Looking for the rules?")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Open the Brand book" })).toHaveCount(0);
    const items = await menu();
    await expect(items.filter({ hasText: "Brand book" })).toHaveCount(0);
    await expect(items.filter({ hasText: "Security" })).toHaveCount(1);
    await page.keyboard.press("Escape");
    await palette("custom module");
    await expect(page.getByRole("option", { name: /How to write a custom module/ })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await page.goto(`${base}/#/customize/modules?tab=modules`);
    await expect(page.getByRole("heading", { name: "Modules", level: 2 })).toBeVisible();
    await expect(page.getByRole("link", { name: "How to write a custom module?" })).toHaveCount(0);
    await page.goto(`${base}/#/customize/build?tab=build`);
    await expect(page.getByText("Describe a model and install it as a module.")).toBeVisible();
    await expect(page.getByRole("link", { name: "How to write a custom module?" })).toHaveCount(0);

    // The addresses of the two hidden tabs open the first tab that is shown.
    for (const hidden of ["guide", "brand"]) {
      await page.goto(`${base}/#/customize/${hidden}?tab=${hidden}`);
      await expect(page).toHaveURL(/tab=theme/);
      await expect(tab("Theme")).toHaveAttribute("aria-selected", "true");
    }
    await page.goto(`${base}/#/customize/guide?tab=guide&section=anything`);
    await expect(page).toHaveURL(/tab=theme/);

    // And the switch is read back, so the Identity tab shows it on.
    await page.goto(`${base}/#/customize/identity?tab=identity`);
    await expect(
      page.getByRole("switch", { name: "Hide the Brand book and the Guide" }),
    ).toBeChecked();
  });

  test("a person who is not an administrator loses them too", async ({
    page,
    request,
    allowHttpErrors,
  }) => {
    allowHttpErrors(404);
    expect((await hide(request, false)).status).toBe(200);
    await openAs(page, `${base}/#/customize/brand?tab=brand`, sam, samName);
    await expect(page.getByRole("tab", { name: "Brand book", selected: true })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Guide" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Identity" })).toHaveCount(0);

    expect((await hide(request, true)).status).toBe(200);
    await page.reload();
    await expect(page.locator(".topbar")).toBeVisible();
    await page.locator("button.avatar-btn").click();
    await expect(page.getByRole("menuitem", { name: "Brand book" })).toHaveCount(0);
    await expect(page.getByRole("menuitem", { name: "Theme" })).toBeVisible();
    await page.keyboard.press("Escape");
    for (const hidden of ["brand", "guide"]) {
      await page.goto(`${base}/#/customize/${hidden}?tab=${hidden}`);
      await expect(page).toHaveURL(/tab=theme/);
      await expect(page.getByRole("tab", { name: "Theme", selected: true })).toBeVisible();
    }
    await expect(page.getByRole("tab", { name: "Brand book" })).toHaveCount(0);
    await expect(page.getByRole("tab", { name: "Guide" })).toHaveCount(0);
    await expect(page.getByRole("tab", { name: "Security" })).toBeVisible();
  });
});

/**
 * Saves the Identity form (with the button, or with Enter in the field that has the focus) and
 * waits until the form was read again. The toast of an earlier save may still be on screen, so a
 * toast proves nothing about this one; the read that follows a save is what puts the form back to
 * the record, and a field filled in before it arrives would be lost.
 */
async function saveIdentity(page: Page, how: "button" | "enter" = "button") {
  const reread = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === "/api/instance_brand/get" && r.request().method() === "POST",
  );
  if (how === "enter") await page.keyboard.press("Enter");
  else await page.getByRole("button", { name: "Save" }).click();
  await reread;
  await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();
}

test.describe("the Identity tab", () => {
  test.describe.configure({ mode: "serial" });
  const base = brandUrl();
  let admin = "";
  let original = "";

  test.beforeAll(async ({ playwright }) => {
    const request = await playwright.request.newContext();
    try {
      admin = await tokenOf(request, base, users.root);
      original = (await call(request, base, admin, "instance_brand.get")).json.value.v as string;
    } finally {
      await request.dispose();
    }
  });
  test.afterAll(async ({ playwright }) => {
    const request = await playwright.request.newContext();
    try {
      await call(request, base, admin, "instance_brand.configure", {
        name: brand.name,
        message_en: brand.message,
        logo_svg: brand.artwork,
        hide_product_docs: false,
      });
    } finally {
      await request.dispose();
    }
  });

  test("an administrator reads and changes the record, and only get and configure are called", async ({
    page,
    request,
    allowHttpErrors,
  }) => {
    test.setTimeout(60_000);
    allowHttpErrors(400); // the refusal of a value that is not a slug, below
    const brandCalls: string[] = [];
    page.on("request", (r) => {
      const m = /\/api\/instance_brand\/([a-z_]+)/.exec(new URL(r.url()).pathname);
      if (m) brandCalls.push(m[1]!);
    });
    await openAs(page, `${base}/#/customize/identity?tab=identity`, admin, users.root.name);
    await expect(page.getByRole("tab", { name: "Identity", selected: true })).toBeVisible();
    // The notice that the text is public, the form as stored, and the preview of the card.
    await expect(page.getByText("Shown before anyone signs in")).toBeVisible();
    await expect(page.getByText(/can be read by anyone who opens the sign-in page/)).toBeVisible();
    const messages = page.getByRole("group", { name: "Message on the sign-in screen" });
    await expect(page.getByRole("textbox", { name: "Name", exact: true })).toHaveValue(brand.name);
    await expect(messages.getByLabel("English")).toHaveValue(brand.message);
    const card = page.getByRole("group", { name: "Preview of the sign-in screen" });
    await expect(card).toContainText(brand.name);
    await expect(card).toContainText(brand.message);
    await expect(card.locator("img")).toHaveAttribute("src", /^\/api\/brand\/logo\.svg\?v=/);
    await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();

    // A message, typed and saved: the card follows the field, and the server holds it.
    const message = `${brand.message}, and welcome back`;
    await messages.getByLabel("English").fill(message);
    await expect(card).toContainText(message);
    await saveIdentity(page, "enter"); // Enter in the field saves, as the button does
    await expect(page.getByText(/^Identity saved\./).last()).toBeVisible();
    const got = (await call(request, base, admin, "instance_brand.get")).json.value;
    expect(got.message_en).toBe(message);
    expect(got.name).toBe(brand.name);

    // A new logo from a file: the version of the logo changes and the preview takes it.
    const logo = brand.artwork.replace("#495670", "#c2410c");
    await page
      .getByLabel("SVG file")
      .setInputFiles({ name: "logo.svg", mimeType: "image/svg+xml", buffer: Buffer.from(logo) });
    await expect(page.getByText("logo.svg will replace the logo when you save.")).toBeVisible();
    await saveIdentity(page);
    await expect
      .poll(async () => (await call(request, base, admin, "instance_brand.get")).json.value.v)
      .not.toBe(original);

    // The switch is set through configure and read through get.
    // The visible text of the switch is clicked, as a person does: the input itself is a hidden box.
    const hide = page.getByRole("switch", { name: "Hide the Brand book and the Guide" });
    const hideLabel = page.locator("label.switch", {
      hasText: "Hide the Brand book and the Guide",
    });
    await hideLabel.click();
    await expect(hide).toBeChecked();
    await saveIdentity(page);
    await expect
      .poll(
        async () =>
          (await call(request, base, admin, "instance_brand.get")).json.value.hide_product_docs,
      )
      .toBe(true);
    // The page that saved it hides the documents at once: the account menu has no Brand book.
    await page.locator("button.avatar-btn").click();
    await expect(page.getByRole("menuitem", { name: "Security" })).toBeVisible();
    await expect(page.getByRole("menuitem", { name: "Brand book" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await hideLabel.click();
    await expect(hide).not.toBeChecked();
    await saveIdentity(page);
    await page.locator("button.avatar-btn").click();
    await expect(page.getByRole("menuitem", { name: "Brand book" })).toBeVisible();
    await page.keyboard.press("Escape");

    // A refusal is shown as the server words it, and nothing is saved.
    await page.getByLabel("Short name").fill("9 not a slug");
    await page.getByRole("button", { name: "Save" }).click();
    await expect(
      page.getByRole("alert").filter({ hasText: "The identity was not saved" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Discard changes" }).click();

    // The log says who changed the identity, in words.
    await page.goto(`${base}/#/customize/security?tab=security`);
    const log = page.getByRole("table", { name: "Security log" });
    await expect(
      log.getByRole("row").filter({ hasText: "The identity of the instance changed" }).first(),
    ).toBeVisible();

    // `clear` is held by the unit test of the tab (Identity.test.tsx), not called here: once the
    // record is gone the logo route answers 404 for the logo the page was loaded with, and the
    // console guard of this suite reports a request for it.
    expect(new Set(brandCalls)).toEqual(new Set(["get", "configure"]));
  });
});

test.describe("an instance with no brand", () => {
  test("the sign-in screen and the recovery file are the product's own", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");
    await expect(page).toHaveTitle("obop");
    await expect(page.locator('link[rel~="icon" i]')).toHaveAttribute("href", /favicon\.svg$/);
    await expect(page.getByText("One business, one platform.")).toBeVisible();
    await expect(page.getByRole("img", { name: "obop" }).first()).toBeVisible();
    await expect(page.locator("h1 img")).toHaveCount(0);
  });
});

test.describe("the control-plane app", () => {
  test("keeps naming its recovery file after the product", async ({ page, uid }) => {
    test.setTimeout(60_000);
    await page.goto(`${platformUrl()}/app/`);
    await page.getByLabel("Email").fill(`${uid("owner").replace(/ /g, "-")}@example.test`);
    await page.getByLabel("Password").fill("e2e platform password");
    await page.getByRole("button", { name: "Create account" }).click();
    await expect(page.getByRole("heading", { name: "Your instances" })).toBeVisible();
    await page.getByRole("button", { name: "Security" }).click();
    const codes = await enrol(page);
    expect(codes).toHaveLength(10);
    expect(await downloadName(page, codes)).toBe("obop-recovery-codes.txt");
  });
});
