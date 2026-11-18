import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { createHash, randomBytes } from "node:crypto";
import type { APIRequestContext, Page } from "@playwright/test";
import { listenForCallback } from "../support/callback.ts";
import { users } from "../support/env.ts";
import { expect, open, test, type Api } from "../support/fixtures.ts";
import { audit, brokenWords, type Audit, type TableFinding } from "../support/touch.ts";

// The administration screens on a phone (390 px) and on a small tablet (768 px), both touch screens.
// Each screen is measured from element boxes after it has settled: the page may not scroll sideways,
// a table that is stacked on a phone is one block per row (a cell spans the row), a table that keeps
// its columns on a tablet fits its box, and every control a finger must hit is 44 px in both
// directions (a link inside a sentence is exempt, as in WCAG 2.5.8). People, the Security log and
// the screens of the ordinary apps are held by their own specs.

test.use({ hasTouch: true, isMobile: true });

const SIZES = [
  { name: "phone", width: 390, height: 844 },
  { name: "tablet", width: 768, height: 1024 },
] as const;

/** Every finding that is a control of its own and not a link in a sentence. */
function small(a: Audit): string[] {
  return a.small.filter((s) => !s.inline).map((s) => `${s.sel} "${s.text}" ${s.w}x${s.h}`);
}

function table(a: Audit, caption: string | RegExp): TableFinding {
  const found = a.tables.filter((t) =>
    typeof caption === "string" ? t.caption === caption : caption.test(t.caption),
  );
  expect(found, `a table named ${caption}`).toHaveLength(1);
  return found[0]!;
}

async function settle(page: Page) {
  await expect(page.getByRole("region", { name: "Loading" })).toHaveCount(0, { timeout: 15_000 });
  await expect(page.locator(".content .skeleton")).toHaveCount(0);
}

/** The screen fits: no sideways page, and nothing under 44 px that a finger must hit. The words of
 *  the tables named in `notHeldHere` are not judged (tables that their own specs hold). */
async function fits(page: Page, what: string, notHeldHere: string[] = []): Promise<Audit> {
  const a = await audit(page);
  expect.soft(a.coarse, `${what}: the browser reports a touch pointer`).toBe(true);
  expect.soft(a.pageOverflow, `${what}: the page scrolls sideways`).toBe(0);
  expect.soft(small(a), `${what}: controls under 44 px`).toEqual([]);
  // A table that fits its box can still be unreadable: no word of a header, a label or a cell may
  // be cut by a line break (words of 20 characters or more are exempt).
  for (const caption of new Set(a.tables.map((t) => t.caption).filter(Boolean)))
    if (!notHeldHere.includes(caption))
      expect.soft(await brokenWords(page, caption), `${what}: broken words`).toEqual([]);
  return a;
}

type Cells = { label: string; width: number; row: number }[];

/** The labelled cells of a table: each cell's width against the inner width of its row. */
async function labelled(page: Page, caption: string): Promise<Cells> {
  return page.evaluate((cap) => {
    const table = [...document.querySelectorAll("table")].find(
      (t) => t.querySelector("caption")?.textContent?.trim() === cap,
    );
    const out: { label: string; width: number; row: number }[] = [];
    for (const cell of table?.querySelectorAll<HTMLElement>("tbody > tr > td[data-label]") ?? []) {
      if (getComputedStyle(cell).display === "none") continue;
      const tr = cell.parentElement!;
      const st = getComputedStyle(tr);
      out.push({
        label: cell.getAttribute("data-label")!,
        width: cell.getBoundingClientRect().width,
        row: tr.clientWidth - parseFloat(st.paddingLeft) - parseFloat(st.paddingRight),
      });
    }
    return out;
  }, caption);
}

/** The declarations that make a stacked row what it is, read from the browser's computed style. */
async function stackStyle(page: Page, caption: string) {
  return page.evaluate((cap) => {
    const table = [...document.querySelectorAll("table")].find(
      (t) => t.querySelector("caption")?.textContent?.trim() === cap,
    )!;
    const pick = (el: Element, props: string[]) => {
      const st = getComputedStyle(el);
      return Object.fromEntries(props.map((p) => [p, st.getPropertyValue(p)]));
    };
    const tr = table.querySelector("tbody > tr")!;
    return {
      table: pick(table, ["display"]),
      tbody: pick(table.querySelector("tbody")!, ["display"]),
      thead: pick(table.querySelector("thead")!, ["position", "clip-path", "overflow", "width"]),
      tr: pick(tr, [
        "display",
        "flex-wrap",
        "align-items",
        "row-gap",
        "column-gap",
        "padding",
        "border-bottom-width",
      ]),
      th: pick(tr.querySelector("th")!, [
        "display",
        "order",
        "flex-basis",
        "flex-grow",
        "padding",
        "border-top-width",
        "white-space",
        "font-weight",
        "font-size",
        "text-align",
      ]),
      td: pick(tr.querySelector("td[data-label]")!, [
        "display",
        "order",
        "flex-grow",
        "flex-shrink",
        "min-inline-size",
        "padding",
        "border-top-width",
        "white-space",
        "overflow-wrap",
        "text-wrap-style",
        "text-align",
      ]),
    };
  }, caption);
}

async function receiver(): Promise<{ url: string; close: () => Promise<void> }> {
  const server: Server = createServer((req, res) => {
    req.resume();
    req.on("end", () => res.writeHead(200, { "content-type": "application/json" }).end("{}"));
  });
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as AddressInfo).port;
  return {
    url: `http://127.0.0.1:${port}/hook`,
    close: () => new Promise((ok) => server.close(() => ok())),
  };
}

const installed = async (api: Api) =>
  ((await api.run("root", "module.list")) as { id: string }[]).map((m) => m.id);

// One instance backs the whole run and other specs install the same bundled packages, expecting
// to find them absent: what a test of this spec installs is removed again after that test.
const added: string[] = [];

/** Install a bundled package unless it is there; a busy machine can take longer than the default
 *  10 seconds. The package and whatever it brought with it are noted for removal. */
async function install(api: Api, request: APIRequestContext, id: string) {
  const had = await installed(api);
  if (had.includes(id)) return;
  const r = await request.post("/api/marketplace_listing/install", {
    data: { id },
    headers: { authorization: `Bearer ${await api.token("root")}` },
    timeout: 120_000,
  });
  expect(r.status(), `install ${id}`).toBe(200);
  // The package first, then the ones it needs: a package cannot go before those that need it.
  const brought = (await installed(api)).filter((x) => x !== id && !had.includes(x));
  added.unshift(id, ...brought);
}

test.afterEach(async ({ api }) => {
  const root = await api.token("root");
  for (const id of added.splice(0)) {
    const r = await api.post("/api/module/uninstall", { id }, root);
    expect.soft(r.status, `uninstall ${id}`).toBe(200);
  }
});

/** A connection that has run twice, so the detail has runs and flows to show. */
async function connectionWithRuns(api: Api, hook: string, name: string, syncs = 2) {
  const policies = await api.run("root", "provider_policy.list");
  const body = {
    enabled: true,
    users_may_use: true,
    dpa_recorded: true,
    dpa_reference: "DPA signed for the receiver of the test",
    legal_basis_text: "We send the contact you choose to the receiver you named.",
  };
  const have = policies.find((p: any) => p.provider === "webhook_out");
  if (have) await api.run("root", "provider_policy.update", { id: have.id, ...body });
  else await api.run("root", "provider_policy.create", { provider: "webhook_out", ...body });
  await api.create("root", "party", { name: "Phone party", email: "party@example.test" });
  const made = await api.run("root", "connection.create", {
    name,
    provider: "webhook_out",
    config: JSON.stringify({ url: hook }),
    credentials: JSON.stringify({ signing_secret: "phone-signing-secret-0123456789" }),
    field_allowlist: JSON.stringify({ party: ["name", "email"] }),
  });
  await api.run("root", "consent.create", {
    connection: made.id,
    purpose: "Keep the billing tool up to date with our customers",
    legal_basis: "contract",
  });
  await api.run("root", "connection.connect", { id: made.id });
  for (let i = 0; i < syncs; i++)
    await api.post("/api/connection/sync", { id: made.id }, await api.token("root"));
}

/** An app that signed in as `sam` through the consent page: one device in the Devices table. */
async function deviceSignsIn(
  browserPage: Page,
  request: APIRequestContext,
  baseURL: string,
  name: string,
) {
  const callback = await listenForCallback();
  try {
    const verifier = randomBytes(32).toString("base64url");
    const challenge = createHash("sha256").update(verifier).digest("base64url");
    const reg = await request.post("/oauth/register", {
      data: {
        client_name: "Phone test app",
        obop_client_class: "mobile",
        redirect_uris: [callback.url],
        token_endpoint_auth_method: "none",
        scope: "base.user",
      },
      // A busy machine: this call once took longer than the default 10 seconds.
      timeout: 30_000,
    });
    const clientId = (await reg.json()).client_id as string;
    const resource = `${baseURL}/api`;
    await browserPage.goto(
      `/oauth/authorize?${new URLSearchParams({
        response_type: "code",
        client_id: clientId,
        redirect_uri: callback.url,
        state: "s1",
        code_challenge: challenge,
        code_challenge_method: "S256",
        resource,
        scope: "base.user",
      })}`,
    );
    await browserPage.getByLabel("Username").fill(users.sam.name);
    await browserPage.getByLabel("Password").fill(users.sam.password);
    await browserPage.getByRole("button", { name: "Sign in", exact: true }).click();
    await browserPage.getByRole("button", { name: "Allow" }).click();
    const code = (await callback.next()).searchParams.get("code")!;
    const done = await request.post("/oauth/token", {
      form: {
        grant_type: "authorization_code",
        client_id: clientId,
        redirect_uri: callback.url,
        code,
        code_verifier: verifier,
        resource,
        device_name: name,
        device_platform: "android",
        device_app_version: "1.4.2",
      },
    });
    expect(done.status()).toBe(200);
  } finally {
    await callback.close();
  }
}

for (const size of SIZES) {
  test.describe(`${size.name} (${size.width} px, touch)`, () => {
    test.beforeEach(async ({ page }) => {
      await page.setViewportSize({ width: size.width, height: size.height });
    });

    test("the sign-in page: Forgot your password? is a finger wide", async ({ page }) => {
      await page.goto("/");
      await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeVisible();
      await fits(page, "sign-in");
      // The link sits under the password field, right-aligned: its text ends where the field ends
      // (no padding of its own pushes it inside), and it is still a finger high.
      const linkButton = page.getByRole("button", { name: "Forgot your password?" });
      const link = await linkButton.boundingBox();
      // The edge of the text itself: a padding of the button would hide in its box.
      const textRight = await linkButton.evaluate((el) => {
        const range = document.createRange();
        range.selectNodeContents(el);
        return range.getBoundingClientRect().right;
      });
      const field = await page.getByLabel("Password").boundingBox();
      expect(link, "the link has a box").not.toBeNull();
      expect(field, "the password field has a box").not.toBeNull();
      expect(
        Math.abs(textRight - (field!.x + field!.width)),
        "the right edge of the text of Forgot your password? against the password field's",
      ).toBeLessThanOrEqual(1);
      expect(link!.height, "Forgot your password? is a finger high").toBeGreaterThanOrEqual(43.5);
      await page.getByRole("button", { name: "Forgot your password?" }).click();
      await fits(page, "sign-in, forgot");
    });

    test("the bar, the account menu and the tabs of Customize have finger-sized controls", async ({
      page,
      signIn,
    }) => {
      await signIn("root", "/customize/theme?tab=theme");
      for (const tab of ["theme", "apps", "identity", "modules", "updates", "build", "brand"]) {
        await open(page, `/customize/${tab}?tab=${tab}`);
        await settle(page);
        await fits(page, `customize ${tab}`);
      }
      await open(page, "/");
      await settle(page);
      await fits(page, "home");
      // The account menu: its items and its language switcher are what a finger hits next.
      await page.getByRole("button", { name: "Account" }).click();
      await expect(page.getByRole("menuitem", { name: "Sign out" })).toBeVisible();
      await fits(page, "account menu open");
      await page.keyboard.press("Escape");
    });

    test("the Guide has finger-sized links and Copy buttons", async ({ page, signIn }) => {
      await signIn("root", "/customize/guide?tab=guide");
      await settle(page);
      await fits(page, "guide");
    });

    test("Customize, Security: sessions, tokens and devices", async ({
      page,
      signIn,
      api,
      request,
      baseURL,
      browser,
    }) => {
      test.setTimeout(120_000);
      await api.post(
        "/api/auth/agent-token",
        { scopes: ["base.user"], label: "phone token with a rather long label" },
        await api.token("root"),
      );
      const other = await browser.newContext({ baseURL });
      try {
        await deviceSignsIn(await other.newPage(), request, baseURL!, "Phone with a long name");
      } finally {
        await other.close();
      }
      await signIn("root", "/customize/security?tab=security");
      await expect(page.getByRole("table", { name: "Devices" })).toBeVisible();
      await settle(page);
      // People and the Security log are not this spec's tables (see the head of the file). At
      // 768 px they are still columns, and among the people and events a whole run leaves behind
      // a short name is cut at its hyphen; on a phone both are stacked and judged like the rest.
      const a = await fits(
        page,
        "security",
        size.name === "tablet" ? ["People", "Security log"] : [],
      );
      for (const caption of ["Your sessions", "Agent tokens", "Devices"]) {
        const t = table(a, caption);
        expect.soft(t.scrolls, `${caption} scrolls sideways`).toBe(0);
        // Devices has eight columns: as a table at 768 px its name ran four lines and each date
        // three, so it is cards up to 60rem (stack-md); the other two keep their columns there.
        expect
          .soft(t.stacked, `${caption} is stacked`)
          .toBe(size.name === "phone" || caption === "Devices");
        expect.soft(await brokenWords(page, caption), `${caption}: broken words`).toEqual([]);
      }
      if (size.name === "tablet") {
        // As cards on a tablet the six labelled cells of a device stand three to a line, as on a
        // phone: with the kit's 6rem cells five shared a line and Status stood alone under them.
        const perLine = await page
          .getByRole("table", { name: "Devices" })
          .locator("tbody tr")
          .first()
          .evaluate((tr) => {
            const tops = [...tr.querySelectorAll<HTMLElement>("td[data-label]")].map((td) =>
              Math.round(td.getBoundingClientRect().top),
            );
            return [...new Set(tops)].map((y) => tops.filter((top) => top === y).length);
          });
        expect(perLine, "labelled cells of a device per line").toEqual([3, 3]);
      }
    });

    test("Modules: the list, a module's panel and its dependency tables", async ({
      page,
      signIn,
      api,
      request,
    }) => {
      test.setTimeout(180_000);
      await install(api, request, "timesheets");
      await signIn("root", "/customize/modules?tab=modules");
      await settle(page);
      await fits(page, "modules");
      for (const section of ["overview", "dependencies", "contents", "versions"]) {
        await open(page, `/customize/modules?tab=modules&id=timesheets&section=${section}`);
        await settle(page);
        const a = await fits(page, `module panel ${section}`);
        const ranges = a.tables.filter((t) => t.caption.startsWith("The versions of the core"));
        const versions = a.tables.filter((t) => t.caption === "Versions of this module");
        for (const t of [...ranges, ...versions]) {
          expect.soft(t.scrolls, `${t.caption} scrolls sideways`).toBe(0);
          expect.soft(t.stacked, `${t.caption} is stacked`).toBe(size.name === "phone");
        }
        if (section === "dependencies") expect(ranges).toHaveLength(1);
        if (section === "versions") expect(versions).toHaveLength(1);
      }
      // The links of the dependency table are 44 px high without the hit boxes of two rows
      // lying over each other (a tap between two names must land on one of them).
      await open(page, "/customize/modules?tab=modules");
      await settle(page);
      await page.evaluate(() => {
        for (const d of document.querySelectorAll<HTMLDetailsElement>("details.dep-table-wrap"))
          d.open = true;
      });
      const links = await page.evaluate(() =>
        [...document.querySelectorAll<HTMLElement>(".dep-table-wrap tbody th a")]
          .map((a) => a.getBoundingClientRect())
          .filter((r) => r.width > 0)
          .map((r) => ({ top: r.top + scrollY, bottom: r.bottom + scrollY })),
      );
      expect(links.length, "rows in the dependency table").toBeGreaterThanOrEqual(2);
      for (let i = 1; i < links.length; i++)
        expect
          .soft(links[i]!.top, `link ${i} starts under link ${i - 1}`)
          .toBeGreaterThanOrEqual(links[i - 1]!.bottom - 0.5);
    });

    test("Connectors: a connection's runs and flows, and the processing register", async ({
      page,
      signIn,
      api,
      uid,
    }) => {
      test.setTimeout(120_000);
      const rx = await receiver();
      try {
        const name = uid("Phone hook");
        await connectionWithRuns(api, rx.url, name);
        // Two runs that failed: the receiver's host does not resolve, and the failure text names it.
        await connectionWithRuns(
          api,
          "https://a-receiver-with-a-rather-long-host-name.example.test/hook",
          uid("Phone failing hook"),
        );
        await signIn("root", "/connectors/overview");
        await settle(page);
        const o = await fits(page, "connectors overview");
        // The Status cell carries the failure text: on a phone it has the row to itself.
        const runs = await page.evaluate(() => {
          const table = [...document.querySelectorAll("table")].find(
            (t) => t.querySelector("caption")?.textContent?.trim() === "Latest runs",
          )!;
          return [...table.querySelectorAll<HTMLElement>("tbody > tr")].map((tr) => {
            const cell = tr.querySelectorAll<HTMLElement>("td")[2]!;
            const st = getComputedStyle(tr);
            return {
              text: cell.textContent ?? "",
              width: cell.getBoundingClientRect().width,
              row: tr.clientWidth - parseFloat(st.paddingLeft) - parseFloat(st.paddingRight),
            };
          });
        });
        expect(
          runs.filter((r) => r.text.includes("could not be resolved")).length,
          "failed runs in Latest runs",
        ).toBeGreaterThanOrEqual(2);
        expect.soft(table(o, "Latest runs").scrolls, "Latest runs scrolls sideways").toBe(0);
        if (size.name === "phone")
          for (const r of runs)
            expect
              .soft(r.width, `the Status cell of "${r.text.slice(0, 30)}" spans its row`)
              .toBeGreaterThanOrEqual(r.row - 1);
        await page
          .locator(".cx-card", { hasText: name })
          .getByRole("button", { name: "Open" })
          .click();
        const detail = page.getByRole("dialog", { name });
        await expect(detail.getByRole("table", { name: "Recent runs" })).toBeVisible();
        await expect(detail.getByRole("table", { name: "Recent data flows" })).toBeVisible();
        const a = await fits(page, "connection detail");
        for (const caption of ["Recent runs", "Recent data flows"]) {
          const t = table(a, caption);
          expect.soft(t.scrolls, `${caption} scrolls sideways`).toBe(0);
          expect.soft(t.stacked, `${caption} is stacked`).toBe(size.name === "phone");
        }
        await page.keyboard.press("Escape");

        await open(page, "/connectors/register");
        await expect(page.getByRole("table", { name: "Processing register" })).toBeVisible();
        const r = await fits(page, "processing register");
        const reg = table(r, "Processing register");
        expect.soft(reg.scrolls, "the register scrolls sideways").toBe(0);
        // Cards up to 60rem: one card per connection on a phone and on a tablet.
        expect.soft(reg.stacked, "the register is stacked").toBe(true);
        const cells = await labelled(page, "Processing register");
        expect(cells.length, "labelled cells of the register").toBeGreaterThanOrEqual(7);
        for (const c of cells) {
          if (size.name === "phone")
            expect.soft(c.width, `${c.label} spans its card`).toBeGreaterThanOrEqual(c.row - 1);
          else expect.soft(c.width, `${c.label} is at least 14rem`).toBeGreaterThanOrEqual(223.5);
        }
        if (size.name === "tablet") {
          // The two ranges of the kit's stacked layout are one layout. The register carries both
          // classes, so it cannot show drift between the blocks (at 390 px the later one wins):
          // two fixture tables do, one with only stack-md (measured at 768 px), one with only
          // stack-sm (measured at 390 px). Everything stackStyle reads must be equal.
          await page.evaluate(() => {
            for (const cls of ["stack-md", "stack-sm"]) {
              const t = document.createElement("table");
              t.className = `table ${cls}`;
              t.setAttribute("role", "table");
              t.innerHTML =
                `<caption>Fixture ${cls}</caption><thead><tr><th scope="col">Name</th>` +
                `<th scope="col">Note</th></tr></thead><tbody><tr><th scope="row">One</th>` +
                `<td data-label="Note">A note</td></tr></tbody>`;
              document.body.append(t);
            }
          });
          const wide = await stackStyle(page, "Fixture stack-md");
          await page.setViewportSize({ width: 390, height: 844 });
          await page.waitForTimeout(250);
          const narrow = await stackStyle(page, "Fixture stack-sm");
          expect(wide, "stack-md at 768 px against stack-sm at 390 px").toEqual(narrow);
        }
      } finally {
        await rx.close();
      }
    });

    test("the Drive browser: one block per file on a phone, its links a finger wide", async ({
      page,
      signIn,
      api,
      request,
    }) => {
      test.setTimeout(120_000);
      await install(api, request, "documents");
      await page.route("**/api/drive/**", async (route) => {
        const path = new URL(route.request().url()).pathname;
        if (path.endsWith("/status"))
          return route.fulfill({
            json: { connected: true, may_browse: true, may_write: true, workspace: null },
          });
        if (path.endsWith("/files"))
          return route.fulfill({
            json: {
              files: [
                {
                  id: "f1",
                  name: "Contracts of 2026",
                  folder: true,
                  importable: false,
                  modified: "2026-11-01T10:00:00Z",
                  size: null,
                  web_link: "https://drive.example.test/f1",
                },
                {
                  id: "f2",
                  name: "Supplier agreement final version signed by both parties.pdf",
                  folder: false,
                  importable: true,
                  modified: "2026-11-02T10:00:00Z",
                  size: 123456,
                  web_link: "https://drive.example.test/f2",
                },
              ],
              next: null,
            },
          });
        return route.fulfill({ json: {} });
      });
      await signIn("root", "/documents/drive");
      await expect(page.getByRole("button", { name: "Import into Documents" })).toBeVisible();
      const a = await fits(page, "drive browser");
      const t = table(a, /./);
      expect.soft(t.scrolls, "the Drive table scrolls sideways").toBe(0);
      expect.soft(t.stacked, "the Drive table is stacked").toBe(size.name === "phone");
      // A folder has no size: its empty cell takes no line of a stacked card, and stays a cell
      // of the row on a tablet.
      const emptyCell = await page.evaluate(() => {
        const cell = [...document.querySelectorAll("tbody tr")]
          .find((r) => r.textContent?.includes("Contracts of 2026"))
          ?.querySelector("td:empty");
        return cell ? getComputedStyle(cell).display : "missing";
      });
      expect(emptyCell, "the display of the folder's empty cell").toBe(
        size.name === "phone" ? "none" : "table-cell",
      );
    });
  });
}

for (const size of SIZES) {
  test(`the Brand book's type scale reads at ${size.width} px`, async ({ page, signIn }) => {
    await page.setViewportSize({ width: size.width, height: size.height });
    await signIn("root", "/customize/brand?tab=brand");
    await settle(page);
    await expect(page.getByRole("table", { name: "Type scale" })).toBeVisible();
    const a = await fits(page, "brand foundations");
    expect.soft(table(a, "Type scale").scrolls, "the Type scale scrolls sideways").toBe(0);
    expect(await brokenWords(page, "Type scale"), "words broken in the Type scale").toEqual([]);
    // No sample ends in an ellipsis: a stacked sample wraps, a tablet's fits its column.
    const cut = await page.evaluate(() =>
      [...document.querySelectorAll<HTMLElement>(".scale-sample")]
        .filter((e) => e.scrollWidth > e.clientWidth + 1)
        .map((e) => e.textContent),
    );
    expect(cut, "type scale samples cut by an ellipsis").toEqual([]);
  });
}
