import type { Page } from "@playwright/test";
import { coreVersion, nextMinor, nextPatch } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";
import { field, pkgText } from "../support/market.ts";

// D63: updating the instance itself from inside it. The app instance holds the update token of an
// instance the (fake-provisioner) control plane knows; two releases are published: a patch that
// fits every installed module and a minor that does not.

const UPDATES = "/customize/updates";
const row = (page: Page, version: string) =>
  page.locator(".upd-release").filter({ has: page.locator("strong", { hasText: version }) });

test.describe("Customize > Updates", () => {
  test("shows the running version, the releases with their notes and what the new core means for the installed modules", async ({
    page,
    signIn,
    api,
  }) => {
    // A module written for the current series only, so the minor release is blocked by it.
    await api.run("root", "module.install", {
      manifest: pkgText({ id: "e2euka", version: "0.1.0", fields: field("note") }),
    });
    try {
      await signIn("root", UPDATES);
      await expect(page.getByRole("heading", { name: "Updates", level: 2 })).toBeVisible();
      await expect(page.getByText(`Core ${coreVersion}`, { exact: true }).first()).toBeVisible();
      await expect(
        page.getByRole("heading", { name: `Version ${nextMinor} is available` }),
      ).toBeVisible();
      await expect(page.getByText("Managed by a control plane")).toBeVisible();
      // The newest release cannot be installed yet; the patch can.
      await expect(page.getByText("The newest release cannot be installed here yet")).toBeVisible();
      const minor = row(page, nextMinor);
      await expect(minor).toContainText("Blocked");
      await minor.locator("summary").click();
      await expect(minor).toContainText("e2euka 0.1.0 would not run");
      await expect(minor.getByRole("table")).toContainText("Would not run");
      await expect(minor.getByRole("table")).toContainText(
        `^${coreVersion.split(".").slice(0, 2).join(".")}`,
      );
      const patch = row(page, nextPatch);
      await expect(patch).toContainText("Can be installed here");
      await expect(patch).toContainText("Bug fixes and small improvements.");
      await expect(page.getByRole("button", { name: "Update now" })).toBeVisible();
      // By-hand instructions are there for people who prefer them.
      await expect(page.getByText("Prefer to update by hand?")).toBeVisible();
      await expect(page.getByText(/docker pull ghcr\.io\/spacecorps\/obop/)).toBeVisible();
    } finally {
      await api.run("root", "module.uninstall", { id: "e2euka" });
    }
  });

  test("updates the instance: backup, request, live progress and the result; then offers the way back", async ({
    page,
    signIn,
    api,
  }) => {
    // With a module written for this series installed, the patch is the newest release that fits.
    await api.run("root", "module.install", {
      manifest: pkgText({ id: "e2eukb", version: "0.1.0" }),
    });
    test.info().annotations.push({ type: "cleanup", description: "e2eukb" });
    await signIn("root", UPDATES);
    await expect(
      page.getByRole("heading", { name: `Version ${nextMinor} is available` }),
    ).toBeVisible();
    await expect(page.getByText(/Backups go to/)).toBeVisible();
    await page.getByRole("button", { name: "Update now" }).click();
    const dialog = page.getByRole("dialog", { name: `Update to ${nextPatch}?` });
    await expect(dialog).toContainText(`${coreVersion} becomes ${nextPatch}`);
    await expect(dialog).toContainText("A backup is taken first");
    await expect(dialog).toContainText("migrated forward");
    await dialog.getByRole("button", { name: "Update now" }).click();

    // Progress is visible while the control plane redeploys, then the result.
    const progress = page.getByRole("region", { name: "Progress of the update" });
    await expect(progress).toBeVisible();
    await expect(progress).toContainText(`Updating from ${coreVersion} to ${nextPatch}`);
    await expect(progress.getByText("Running")).toBeVisible();
    await expect(progress.getByText("Done")).toBeVisible({ timeout: 20_000 });
    await expect(progress).toContainText("Backup taken first");
    await expect(progress.locator("code")).toContainText("obop-backup-");

    // The request and its result are in the audit log.
    const kinds: string[] = (await api.run("root", "security_event.list", { limit: 1000 })).map(
      (e: { kind: string }) => e.kind,
    );
    expect(kinds).toContain("instance_update_requested");
    expect(kinds).toContain("instance_update_finished");
    const status = await api.run("root", "instance_update.status");
    expect(status.state).toBe("succeeded");
    expect(status.target_version).toBe(nextPatch);

    // This process still reports the old version in the test (nothing restarts it), so the page
    // says the control plane finished and the instance restarts on the new version.
    await expect(progress).toContainText("restarts on the new version");
    await api.run("root", "module.uninstall", { id: "e2eukb" });
  });

  test("an agent can only propose an update and a non-admin sees nothing of it", async ({
    api,
    allowHttpErrors,
  }) => {
    allowHttpErrors(403, 404);
    const root = await api.token("root");
    const agent = await api.post(
      "/api/auth/agent-token",
      { scopes: ["platform.admin"], label: "updater" },
      root,
    );
    expect(agent.status).toBe(200);
    for (const [path, body] of [
      ["/api/instance_update/start", { version: nextPatch, skip_backup: true }],
      ["/api/instance_update/rollback", {}],
    ] as const) {
      const r = await api.post(path, body, agent.json.token);
      expect(JSON.stringify(r.json), path).toMatch(/plan_id|approval/);
    }
    const sam = await api.token("sam");
    for (const path of ["/api/instance_update/check", "/api/instance_update/start"]) {
      const r = await api.post(path, { version: nextPatch }, sam);
      expect([403, 404], path).toContain(r.status);
    }
  });

  test("the tab is for administrators only", async ({ page, signIn }) => {
    await signIn("sam", "/customize/theme");
    await expect(page.getByRole("tab", { name: "Updates" })).toHaveCount(0);
  });
});
