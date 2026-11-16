import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync } from "node:fs";
import { createServer } from "node:net";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { backofficeDist, obopBinary, secretKey } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { base32Decode, hotp, STEP } from "../support/totp.ts";

// D96 (PROPOSED): the first administrator of a new instance, made in the browser. The spec starts
// an EMPTY instance of its own (no `obop admin create`), with a server key and the default policy
// (two-factor required for administrators), reads the set-up code from what the server writes to
// its standard error, and does what a person on a phone would do: the set-up page at 390 px, the
// forced two-factor enrolment, the launcher. A second visit shows the normal sign-in form.

const PASSWORD = "a long enough password 1";
const NAME = "Acme Test";

type Instance = { child: ChildProcess; origin: string; log: () => string };
let instance: Instance;

async function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const s = createServer();
    s.once("error", fail);
    s.listen(0, "127.0.0.1", () => {
      const a = s.address();
      s.close(() => (a && typeof a === "object" ? ok(a.port) : fail(new Error("no port"))));
    });
  });
}

async function startEmpty(): Promise<Instance> {
  const dir = join(process.env.OBOP_E2E_DIR!, "setup");
  mkdirSync(dir, { recursive: true });
  const port = await freePort();
  const origin = `http://127.0.0.1:${port}`;
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    OBOP_SECRET_KEY: secretKey,
    OBOP_PUBLIC_URL: origin,
    OBOP_DEFAULT_LANGUAGE: "",
  };
  // The default policy: an administrator must set up two-factor before anything else.
  delete env.OBOP_REQUIRE_MFA_FOR_ADMINS;
  const child = spawn(
    obopBinary(),
    [
      "serve",
      "--addr",
      `127.0.0.1:${port}`,
      "--db",
      `sqlite://${join(dir, "setup.sqlite")}`,
      "--static",
      backofficeDist,
    ],
    { env, stdio: ["ignore", "ignore", "pipe"] },
  );
  let log = "";
  child.stderr!.on("data", (d: Buffer) => (log += d.toString()));
  const deadline = Date.now() + 60_000;
  for (;;) {
    if (child.exitCode !== null) throw new Error(`obop serve exited:\n${log}`);
    const up = await fetch(`${origin}/healthz`)
      .then((r) => r.ok)
      .catch(() => false);
    if (up) break;
    if (Date.now() > deadline) throw new Error(`obop serve did not start:\n${log}`);
    await new Promise((r) => setTimeout(r, 200));
  }
  return { child, origin, log: () => log };
}

/** The code the server printed: the only place it is ever shown. */
function setupCode(i: Instance): string {
  const m = /^Set-up code: ([A-Z0-9-]+)\s*$/m.exec(i.log());
  if (!m) throw new Error(`no set-up code in the server output:\n${i.log()}`);
  return m[1]!;
}

async function noSidewaysScroll(page: Page) {
  const wide = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
  expect(wide, "sideways scroll").toBe(false);
}

async function fill(page: Page, code: string, repeat = PASSWORD) {
  await page.getByLabel("Name of this instance (optional)").fill(NAME);
  await page.getByLabel("Set-up code").fill(code);
  await page.getByLabel("Username").fill("root");
  await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
  await page.getByLabel("Repeat the password").fill(repeat);
}

test.describe.configure({ mode: "serial" });
test.use({ viewport: { width: 390, height: 844 } });

test.beforeAll(async () => {
  instance = await startEmpty();
});

test.afterAll(async () => {
  instance?.child.kill();
});

test("an empty instance prints a code and shows the set-up page, not a sign-in that cannot work", async ({
  page,
}) => {
  expect(instance.log()).toContain("No users yet. Open this instance in a browser");
  expect(instance.log()).toContain("From a shell: obop admin create NAME --all-groups");
  const code = setupCode(instance);
  await page.goto(`${instance.origin}/`);
  await expect(page.getByRole("heading", { name: "Set up this instance" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toHaveCount(0);
  await noSidewaysScroll(page);
  for (const w of [320, 390]) {
    await page.setViewportSize({ width: w, height: 800 });
    await noSidewaysScroll(page);
    const box = await page.getByRole("button", { name: "Create the administrator" }).boundingBox();
    expect(box!.height, `button at ${w} px`).toBeGreaterThanOrEqual(44);
    for (const label of ["Set-up code", "Username", "Repeat the password"]) {
      const b = await page.getByLabel(label).boundingBox();
      expect(b!.height, `${label} at ${w} px`).toBeGreaterThanOrEqual(44);
      expect(b!.x + b!.width).toBeLessThanOrEqual(w);
    }
  }
  // The status never carries the code.
  const status = await (await fetch(`${instance.origin}/api/auth/setup`)).text();
  expect(JSON.parse(status)).toEqual({ open: true, code_active: true, needs_secret_key: false });
  expect(status).not.toContain(code.slice(0, 5));
});

test("what the form can tell by itself never reaches the server, and a wrong code is told plainly", async ({
  page,
  allowHttpErrors,
}) => {
  allowHttpErrors(403);
  await page.goto(`${instance.origin}/`);
  let posts = 0;
  page.on("request", (r) => {
    if (r.method() === "POST" && r.url().endsWith("/api/auth/setup")) posts++;
  });
  await fill(page, setupCode(instance), "another long password 2");
  await page.getByRole("button", { name: "Create the administrator" }).click();
  await expect(page.getByText("The two passwords are not the same.")).toBeVisible();
  await expect(page.getByLabel("Repeat the password")).toBeFocused();
  expect(posts).toBe(0);

  await fill(page, "WRONG-WRONG-WRONG-WRONG");
  await page.getByRole("button", { name: "Create the administrator" }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toContainText(
    "The set-up code is not right, or it has run out. Check the log of the server, or restart it to print a new code.",
  );
  await expect(page.locator(".setup-alert")).toBeFocused();
  expect(posts).toBe(1);
});

test("the right code creates the administrator, who must set up two-factor and then reaches the launcher", async ({
  page,
}) => {
  await page.goto(`${instance.origin}/`);
  await fill(page, setupCode(instance));
  await page.getByRole("button", { name: "Create the administrator" }).click();
  await expect(page.getByText("Set up two-factor authentication to continue")).toBeVisible();
  await expect(page.locator(".topbar")).toHaveCount(0);
  await noSidewaysScroll(page);

  // The authenticator entry carries the name typed on the set-up page.
  const begun = page.waitForResponse((r) => r.url().endsWith("/api/auth/mfa/begin"));
  await page.getByRole("button", { name: "Set up two-factor" }).click();
  const b = (await (await begun).json()) as { issuer: string; uri: string };
  expect(b.issuer).toBe(NAME);
  expect(b.uri).toContain(`issuer=${encodeURIComponent(NAME)}`);
  const secret = ((await page.getByLabel("Setup key").textContent()) ?? "").replace(/\s/g, "");
  await page
    .getByLabel("6-digit code")
    .fill(hotp(base32Decode(secret), Math.floor(Date.now() / 1000 / STEP)));
  await page.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByRole("heading", { name: "Save your recovery codes" })).toBeVisible();
  await page.getByLabel("I have saved these codes").check();
  await page.getByRole("button", { name: "Done" }).click();
  await expect(page.locator(".topbar")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Your apps" })).toBeVisible();
  await noSidewaysScroll(page);
});

test("a second visit shows the normal sign-in form, and the set-up is closed for good", async ({
  page,
}) => {
  // A new browser context: nothing of the earlier session is left.
  await page.goto(`${instance.origin}/`);
  await expect(page.getByRole("heading", { name: "Sign in", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Set up this instance" })).toHaveCount(0);
  // The instance name chosen at set-up is the brand the sign-in page shows.
  await expect(page.getByRole("heading", { name: NAME }).first()).toBeVisible();
  const status = await (await fetch(`${instance.origin}/api/auth/setup`)).json();
  expect(status).toEqual({ open: false });
  const again = await fetch(`${instance.origin}/api/auth/setup`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code: setupCode(instance), username: "late", password: PASSWORD }),
  });
  expect(again.status).toBe(409);
});
