import type { Page } from "@playwright/test";
import { platformUrl } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

const PASSWORD = "e2e platform password";

async function signUp(page: Page, email: string) {
  await page.goto(`${platformUrl()}/app/`);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page.getByRole("heading", { name: "Your instances" })).toBeVisible();
}

const emailFor = (uid: (p: string) => string) => `${uid("owner").replace(/ /g, "-")}@example.test`;
// 3 to 30 characters; the slug comes from the name.
const nameFor = (uid: (p: string) => string) => uid("Acme").replace(/ /g, "").slice(0, 24);

test.describe("landing page", () => {
  test("loads, shows the pitch and its call to action reaches the app", async ({ page }) => {
    await page.goto(`${platformUrl()}/`);
    await expect(page).toHaveTitle(/obop/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("One business.");
    await expect(page.getByRole("link", { name: "See the apps" })).toBeVisible();

    await page.getByRole("link", { name: "Get your own instance" }).first().click();
    await expect(page).toHaveURL(`${platformUrl()}/app/`);
    await expect(page.getByRole("button", { name: "Create account" })).toBeVisible();
  });
});

test.describe("control plane (fake provisioner)", () => {
  test("sign up, create an instance and see it running with its address", async ({ page, uid }) => {
    await signUp(page, emailFor(uid));
    await expect(page.getByText("You have no instances yet.")).toBeVisible();

    const name = nameFor(uid);
    await page.getByLabel("New instance").fill(name);
    await page.getByRole("button", { name: "Create" }).click();

    const item = page.locator("ul.instances li", { hasText: name });
    await expect(item.getByRole("link", { name: /^https:\/\/.*\.fake\.test$/ })).toBeVisible();
    await expect(item.getByRole("button", { name: "Delete" })).toBeEnabled();
    await expect(item.getByRole("status")).toHaveCount(0);
  });

  test("while an instance is being set up the page shows the live phase and steps, then running", async ({
    page,
    uid,
  }) => {
    await signUp(page, emailFor(uid));
    // The fake provisioner finishes instantly, so hold the list in "provisioning" to see the progress UI.
    let hold = true;
    await page.route("**/api/platform/instances", async (route) => {
      if (route.request().method() !== "GET" || !hold) return route.continue();
      const res = await route.fetch();
      const body = await res.json();
      body.instances = body.instances.map((i: any) => ({
        ...i,
        status: "provisioning",
        phase: "Building the image",
      }));
      await route.fulfill({ response: res, json: body });
    });

    const name = nameFor(uid);
    await page.getByLabel("New instance").fill(name);
    await page.getByRole("button", { name: "Create" }).click();
    const item = page.locator("ul.instances li", { hasText: name });
    const progress = item.getByRole("status");
    await expect(progress).toContainText("Building the image");
    await expect(progress.locator("li.done")).toHaveText([/Database/, /Admin account/, /Service/]);
    await expect(progress.locator("li.now")).toHaveText(/Build and start/);
    await expect(item.getByRole("button", { name: "Delete" })).toBeDisabled();

    hold = false;
    await expect(item.getByRole("link", { name: /\.fake\.test$/ })).toBeVisible({
      timeout: 15_000,
    });
    await expect(progress).toHaveCount(0);
  });

  test("deleting answers at once with 'deleting' and the instance then disappears", async ({
    page,
    uid,
  }) => {
    await signUp(page, emailFor(uid));
    const name = nameFor(uid);
    await page.getByLabel("New instance").fill(name);
    await page.getByRole("button", { name: "Create" }).click();
    const item = page.locator("ul.instances li", { hasText: name });
    await expect(item.getByRole("link", { name: /\.fake\.test$/ })).toBeVisible();

    // The confirmation is an in-app dialog, not window.confirm: it names the instance, Cancel keeps it.
    await item.getByRole("button", { name: /Delete/ }).click();
    const dialog = page.getByRole("alertdialog", { name: "Delete instance?" });
    await expect(dialog).toContainText(name);
    await expect(dialog).toContainText("This cannot be undone");
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(item.getByRole("link", { name: /\.fake\.test$/ })).toBeVisible();

    const deleted = page.waitForResponse((r) => /\/instances\/[^/]+\/delete$/.test(r.url()));
    const started = Date.now();
    await item.getByRole("button", { name: /Delete/ }).click();
    await page
      .getByRole("alertdialog", { name: "Delete instance?" })
      .getByRole("button", { name: "Delete instance" })
      .click();
    const res = await deleted;
    expect(res.status()).toBe(202);
    expect((await res.json()).status).toBe("deleting");
    expect(Date.now() - started, "the delete call does not wait for the teardown").toBeLessThan(
      2_000,
    );

    await expect(page.locator("ul.instances li", { hasText: name })).toHaveCount(0);
    await expect(page.getByText("You have no instances yet.")).toBeVisible();
  });

  test("an account can sign out and back in, and a bad password is refused", async ({
    page,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(401);
    const email = emailFor(uid);
    await signUp(page, email);
    await page.getByRole("button", { name: "Sign out" }).click();
    await page.getByRole("button", { name: "I already have an account" }).click();

    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill("definitely the wrong one");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("alert")).toBeVisible();

    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page.getByRole("heading", { name: "Your instances" })).toBeVisible();
  });
});
