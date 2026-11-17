import { expect, test, type Api } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";
import {
  ADMIN,
  buildAccess,
  groupsOfModule,
  type ModuleLike,
} from "../../../apps/backoffice/src/access.ts";

// Access by app (D69 amendment, PROPOSED): People shows what a person may use as one row per app.
// This spec runs access.ts over the real module list of a running instance with every bundled
// module installed (the manifests are not parsed here: the server's own `module.list` is the
// truth the screen reads), and checks the table and the dialog at laptop and phone widths.

const SECURITY = "/customize/security?tab=security";

type Meta = { modules: { id: string; name: string; launcher?: boolean }[] };

/** Uninstall what was installed, dependents first: a module that others need is refused until they go. */
async function uninstall(api: Api, ids: string[]) {
  let left = [...ids];
  for (let pass = 0; pass < 8 && left.length > 0; pass++) {
    const still: string[] = [];
    for (const id of left) {
      const r = await api.post("/api/module/uninstall", { id }, await api.token("root"));
      if (r.status !== 200) still.push(id);
    }
    left = still;
  }
  return left;
}

test.describe("access by app", () => {
  test("every group of every bundled module lands in exactly one place, and every app with groups has a base", async ({
    api,
    allowHttpErrors,
  }, info) => {
    test.setTimeout(240_000);
    allowHttpErrors(400, 404, 409, 422);
    const root = await api.token("root");
    const before = (await api.run("root", "module.list")).map((m: { id: string }) => m.id);
    const offered = (await api.run("root", "marketplace_listing.list")) as {
      id: string;
      installed?: boolean;
    }[];
    const added: string[] = [];
    const refused: string[] = [];
    try {
      for (const m of offered) {
        if (before.includes(m.id)) continue;
        const r = await api.post("/api/marketplace_listing/install", { id: m.id }, root);
        if (r.status === 200) added.push(m.id);
        else refused.push(`${m.id} (${r.status})`);
      }
      const rows = (await api.run("root", "module.list")) as ModuleLike[];
      const known = ((await api.get("/api/auth/groups", root)).json.groups ?? []) as string[];
      const meta = (await api.get("/api/meta", root)).json as Meta;
      // The launcher the browser draws: every module that has a place on it, in the order given.
      const apps = meta.modules
        .filter((m) => m.id !== "platform" && m.launcher !== false)
        .map((m) => ({ id: m.id, name: m.name }));
      const access = buildAccess(rows, apps, known);

      // Every group any module names lands in exactly one place: base, manage, an extra, or other.
      const named = new Set(rows.flatMap((r) => groupsOfModule(r)));
      named.delete(ADMIN);
      const placed = new Map<string, number>();
      const put = (g: string | null) => g && placed.set(g, (placed.get(g) ?? 0) + 1);
      for (const l of access.ladders) {
        put(l.base);
        put(l.manage);
        for (const g of l.extras) put(g);
      }
      for (const g of access.other) put(g);
      const lost = [...named].filter((g) => !placed.has(g));
      const twice = [...placed].filter(([, n]) => n !== 1).map(([g]) => g);
      expect({ lost, twice }).toEqual({ lost: [], twice: [] });

      // Every launcher app with groups has a base, and a manage group is never its own base.
      const launcherWithGroups = apps.filter((a) =>
        rows.some((r) => r.id === a.id && groupsOfModule(r).length > 0),
      );
      const ladders = new Map(access.ladders.map((l) => [l.id, l]));
      for (const a of launcherWithGroups) {
        const l = ladders.get(a.id);
        // An app whose every group belongs to another app (a chart naming the accounting groups)
        // has no ladder of its own: its groups are on the ladder of the owner.
        if (!l) {
          const owners = access.ladders.filter((x) =>
            groupsOfModule(rows.find((r) => r.id === a.id)!).every((g) => x.groups.includes(g)),
          );
          expect(owners.length, `${a.id} has groups but no ladder and no owner`).toBeGreaterThan(0);
          continue;
        }
        expect(l.base, `${a.id} has a base`).toBeTruthy();
        expect(l.manage).not.toBe(l.base);
      }

      // The modules whose ladder is not User and Manager, for the hand-over note.
      const odd = access.ladders
        .filter(
          (l) =>
            l.base.split(".").slice(1).join(".") !== "user" ||
            (l.manage !== null && l.manage.split(".").slice(1).join(".") !== "manager"),
        )
        .map((l) => ({ app: l.id, base: l.base, manage: l.manage, extras: l.extras }));
      const single = access.ladders.filter((l) => l.manage === null).map((l) => l.id);
      const summary = { installed: added, refused, odd, single, other: access.other };
      console.log(`ACCESS-LADDERS ${JSON.stringify(summary)}`);
      await info.attach("ladders.json", {
        body: JSON.stringify(summary, null, 2),
        contentType: "application/json",
      });
      expect(access.ladders.length).toBeGreaterThan(5);
    } finally {
      expect(await uninstall(api, added), "modules this test installed are removed again").toEqual(
        [],
      );
    }
  });

  test("the People table fits its box at 1280 px with every column on", async ({
    page,
    signIn,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await signIn("root", SECURITY);
    const people = page.getByRole("table", { name: "People" });
    await expect(people).toBeVisible();
    await expect(people.getByRole("columnheader", { name: "Access" })).toBeVisible();
    await expect(people.getByRole("columnheader", { name: "Role" })).toHaveCount(0);
    await expect(people.getByRole("columnheader", { name: "Groups" })).toHaveCount(0);
    const widths = await people.evaluate((table) => {
      const box = table.closest<HTMLElement>(".table-wrap") ?? table.parentElement!;
      return { scrollWidth: box.scrollWidth, clientWidth: box.clientWidth };
    });
    expect(widths.scrollWidth).toBeLessThanOrEqual(widths.clientWidth);
    expect(await layoutProblems(page)).toEqual([]);
  });

  test("the menu More opens by keyboard and returns the focus; Access opens and returns the focus", async ({
    page,
    signIn,
  }) => {
    await signIn("root", SECURITY);
    const people = page.getByRole("table", { name: "People" });
    const row = people.getByRole("row").filter({
      has: page.getByRole("cell", { name: "sam", exact: true }),
    });
    const more = row.getByRole("button", { name: "More actions for sam" });
    await more.focus();
    await page.keyboard.press("ArrowDown");
    const items = page.getByRole("menuitem");
    await expect(items.first()).toBeFocused();
    await expect(items).toHaveText(["Reset password", "Disable", "Offboard"]);
    await page.keyboard.press("ArrowDown");
    await expect(items.nth(1)).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(more).toBeFocused();

    const open = row.getByRole("button", { name: "Access of sam" });
    await open.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Access of sam" });
    await expect(dialog).toBeVisible();
    // The choices of an app are a radio group: arrow keys move and choose, Tab leaves the group.
    const sales = dialog.getByRole("radiogroup", { name: "Access to Sales" });
    await expect(sales.getByRole("radio", { name: "User", exact: true })).toBeChecked();
    await sales.getByRole("radio", { name: "User", exact: true }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(sales.getByRole("radio", { name: "Manager", exact: true })).toBeChecked();
    await page.keyboard.press("ArrowLeft");
    await expect(dialog.getByRole("button", { name: "Save access" })).toBeDisabled();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(open).toBeFocused();
  });

  test("on a phone: the row stacks with 44 px targets, the sheet changes a level and nothing scrolls sideways", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const name = uid("phone").replace(/ /g, "-");
    const made = await api.post(
      "/api/auth/users",
      {
        username: name,
        password: "e2e phone person pw 7!",
        groups: ["sales.user", "api.device_admin"],
      },
      await api.token("root"),
    );
    expect(made.status).toBe(200);
    await page.setViewportSize({ width: 390, height: 844 });
    await signIn("root", SECURITY);
    const row = page
      .getByRole("table", { name: "People" })
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name, exact: true }) });
    await row.scrollIntoViewIfNeeded();
    await expect(row).toContainText("Sales · User");
    expect(await layoutProblems(page)).toEqual([]);
    const open = row.getByRole("button", { name: `Access of ${name}` });
    const more = row.getByRole("button", { name: `More actions for ${name}` });
    for (const b of [open, more])
      expect((await b.boundingBox())!.height).toBeGreaterThanOrEqual(44);

    await more.click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    const box = (await menu.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    await page.keyboard.press("Escape");

    await open.click();
    const dialog = page.getByRole("dialog", { name: `Access of ${name}` });
    await expect(dialog).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    const sales = dialog.getByRole("radiogroup", { name: "Access to Sales" });
    for (const r of await sales.getByRole("radio").all())
      expect((await r.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await sales.getByRole("radio", { name: "Manager", exact: true }).click();
    await dialog.getByRole("button", { name: "Save access" }).click();
    await expect(page.getByText(`Access of ${name} saved.`)).toBeVisible();
    await expect(row).toContainText("Sales · Manager");
    // The group of no app was carried over: the server still has it.
    const stored = await api.get("/api/auth/users", await api.token("root"));
    const groups = (stored.json.users as { name: string; groups: string[] }[]).find(
      (u) => u.name === name,
    )!.groups;
    expect(groups.sort()).toEqual(["api.device_admin", "sales.manager", "sales.user"]);
    expect(await layoutProblems(page)).toEqual([]);
    await expect(open).toBeFocused();
  });
});
