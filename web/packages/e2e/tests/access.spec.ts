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

/** Apps whose ladder is not <id>.user and <id>.manager: they are reported, not asserted. */
const ODD_APPS = ["helpdesk", "mileage", "connectors"];

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
      // The server lists a wildcard marker among the groups; it is not a group anyone holds.
      named.delete("*");
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
        // The ladder is the one the id says, unless the app is on the list of odd ones (it is
        // printed below): a module that names <id>.user has it as base, and <id>.manager as the
        // manage rung when any model is written through it.
        if (!ODD_APPS.includes(a.id)) {
          const row = rows.find((r) => r.id === a.id)!;
          const names = new Set(groupsOfModule(row));
          const manager = `${a.id}.manager`;
          if (names.has(`${a.id}.user`)) expect(l.base, `${a.id} base`).toBe(`${a.id}.user`);
          if ((row.contents?.models ?? []).some((m) => m.write_group === manager))
            expect(l.manage, `${a.id} manage`).toBe(manager);
        }
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

  test("the People table fits its box at 1280 px with its widest access cell (the Microsoft column is measured in sso-admin.spec.ts)", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    // A long name, and every app of the instance: three chips and "+N more" in one cell.
    const root = await api.token("root");
    const known = ((await api.get("/api/auth/groups", root)).json.groups ?? []) as string[];
    const many = known.filter((g) => /\.(user|agent)$/.test(g));
    expect(many.length).toBeGreaterThan(4);
    const wide = `${uid("wide").replace(/ /g, "-")}.with.a.very.long.name.of.a.person@example.test`;
    const made = await api.post(
      "/api/auth/users",
      { username: wide, password: "e2e wide person pw 7!", groups: many },
      root,
    );
    expect(made.status).toBe(200);
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
    const widest = people
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: wide, exact: true }) });
    await expect(widest.getByRole("button", { name: /^\+\d+ more, Access of / })).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
  });

  test("the menu More of the last row opens whole, above the button, outside the table's box", async ({
    page,
    signIn,
  }) => {
    await page.setViewportSize({ width: 1280, height: 900 });
    await signIn("root", SECURITY);
    const people = page.getByRole("table", { name: "People" });
    const last = people.getByRole("row").last();
    await last.scrollIntoViewIfNeeded();
    await last.getByRole("button", { name: /^More actions for / }).click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu).toBeInViewport({ ratio: 1 });
    const items = menu.getByRole("menuitem");
    for (const item of await items.all()) await expect(item).toBeInViewport({ ratio: 1 });
    // Not cut by the box of the table: the element at the middle of the last item is the item.
    const hit = await items.last().evaluate((el) => {
      const r = el.getBoundingClientRect();
      const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return top === el || el.contains(top);
    });
    expect(hit).toBe(true);
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

    const open = row.getByRole("button", { name: "Access of sam", exact: true });
    await open.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Access of sam" });
    await expect(dialog).toBeVisible();
    // The choices of an app are a radio group: arrow keys move and choose, Tab leaves the group.
    // (Connectors has a User and an Admin rung; sam holds neither.)
    const rungs = dialog.getByRole("radiogroup", { name: "Access to Connectors" });
    await expect(rungs.getByRole("radio", { name: "No access", exact: true })).toBeChecked();
    await rungs.getByRole("radio", { name: "No access", exact: true }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(rungs.getByRole("radio", { name: "User", exact: true })).toBeChecked();
    await page.keyboard.press("ArrowRight");
    await expect(rungs.getByRole("radio", { name: "Admin", exact: true })).toBeChecked();
    await expect(dialog.getByRole("button", { name: "Save access" })).toBeEnabled();
    await page.keyboard.press("ArrowLeft");
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
    const open = row.getByRole("button", { name: `Access of ${name}`, exact: true });
    const more = row.getByRole("button", { name: `More actions for ${name}` });
    for (const b of [open, more])
      expect((await b.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    // The access has a line of its own (it spans the row), and the "+N more" chip is a 44 px target.
    const cell = (await row.locator("td.access-cell").boundingBox())!;
    const rowBox = (await row.boundingBox())!;
    expect(cell.width).toBeGreaterThan(rowBox.width * 0.8);
    const chip = row.getByRole("button", { name: /^\+\d+ more, Access of / });
    expect((await chip.boundingBox())!.height).toBeGreaterThanOrEqual(44);

    await more.click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    for (const item of await menu.getByRole("menuitem").all())
      // The layout height: the menu scales in as it opens, which would shave a bounding box.
      expect(await item.evaluate((el) => (el as HTMLElement).offsetHeight)).toBeGreaterThanOrEqual(
        44,
      );
    const box = (await menu.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    await page.keyboard.press("Escape");

    await open.click();
    const dialog = page.getByRole("dialog", { name: `Access of ${name}` });
    await expect(dialog).toBeVisible();
    expect(await layoutProblems(page)).toEqual([]);
    const rungs = dialog.getByRole("radiogroup", { name: "Access to Connectors" });
    for (const r of await rungs.getByRole("radio").all())
      expect((await r.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    for (const b of [
      dialog.getByRole("button", { name: "Save access" }),
      dialog.getByRole("button", { name: "Cancel" }),
    ])
      expect((await b.boundingBox())!.height).toBeGreaterThanOrEqual(44);
    await rungs.getByRole("radio", { name: "Admin", exact: true }).click();
    await dialog.getByRole("button", { name: "Save access" }).click();
    await expect(page.getByText(`Access of ${name} saved.`)).toBeVisible();
    await expect(row).toContainText("Connectors · Admin");
    // The group of no app was carried over: the server still has it.
    const stored = await api.get("/api/auth/users", await api.token("root"));
    const groups = (stored.json.users as { name: string; groups: string[] }[]).find(
      (u) => u.name === name,
    )!.groups;
    expect(groups.sort()).toEqual([
      "api.device_admin",
      "connectors.admin",
      "connectors.user",
      "sales.user",
    ]);
    expect(await layoutProblems(page)).toEqual([]);
    await expect(open).toBeFocused();
  });
});
