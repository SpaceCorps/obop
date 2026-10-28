import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { coreSeries } from "../support/env.ts";
import { expect, test } from "../support/fixtures.ts";

// D52 amendment: a text field can declare `shows_option_of = "<model>.<field>"` (with an optional `when`
// condition on the same record), and the back office then draws its value, when it is an option key of that
// select field, with the option's label in the active language. Only what is drawn changes: the API, the
// filters, the sort order and the CSV export keep the stored key.
//
// The fixture is a case with a status select (labelled in Swedish) and a history entry whose `note` holds
// either the status written by an effect (kind "status") or a free note (kind "note"), and whose `role` holds
// a role key copied into the line, with no condition.

const q = JSON.stringify;

const manifest = (id: string) => `
[module]
id = ${q(id)}
version = "0.1.0"
name = "Shows option"
description = "A history whose text holds the keys of a select (end-to-end test module)"
author = "obop e2e"
requires_core = ${q(coreSeries)}

[[model]]
name = "${id}_case"
description = "A case"
read_group = "base.user"
write_group = "${id}.manager"

  [[model.field]]
  name = "status"
  description = "Where the case stands"
  class = "none"
  kind = "select"
  options = ["draft", "pagar", "klar"]

  [[model.field]]
  name = "role"
  description = "Who has the case"
  class = "none"
  kind = "select"
  options = ["owner", "helper"]

[[model]]
name = "${id}_entry"
description = "A history entry"
read_group = "base.user"
write_group = "${id}.manager"

  [[model.field]]
  name = "note"
  description = "The status key written by an effect, or a free note"
  class = "none"
  shows_option_of = "${id}_case.status"
  when = 'kind == "status"'

  [[model.field]]
  name = "kind"
  description = "status or note"
  class = "none"

  [[model.field]]
  name = "role"
  description = "The role key copied into the line"
  class = "none"
  shows_option_of = "${id}_case.role"

  [[model.field]]
  name = "details"
  description = "The same, as a long text"
  class = "none"
  kind = "longtext"
  shows_option_of = "${id}_case.status"
  when = 'kind == "status"'

  [[model.field]]
  name = "tag"
  description = "Names the line in the test"
  class = "none"

[labels.sv.model.${id}_case.field.status.options]
pagar = "Pågår"
klar = "Klar"

[labels.sv.model.${id}_case.field.role.options]
owner = "Ägare"
`;

/** The history lines the specs read: a status key, a free note that reads like one, and what is not an option. */
const lines = [
  { tag: "e1", kind: "status", note: "pagar", details: "pagar", role: "owner" },
  { tag: "e2", kind: "note", note: "klar", details: "klar", role: "owner" },
  { tag: "e3", kind: "status", note: "Fri text", details: "Fri text", role: "stranger" },
  { tag: "e4", kind: "status", note: "klar", details: "klar", role: "helper" },
];

const grid = (page: Page) => page.getByRole("grid").first();
const line = (page: Page, tag: string) => grid(page).getByRole("row").filter({ hasText: tag });

test.describe("a text field that shows the label of an option", () => {
  test.describe("in Swedish", () => {
    test.use({ language: "sv" });

    test("a status key reads as the label, a free note and a non-option as stored, and the API keeps the key", async ({
      page,
      signIn,
      api,
      scratchPackage,
    }) => {
      const id = await scratchPackage(manifest);
      const entry = `${id}_entry`;
      for (const l of lines) await api.create("root", entry, l);

      // What the server says: the attribute is metadata of the text field and of nothing else.
      const meta = (await api.get("/api/meta", await api.token("root"))).json.modules.find(
        (m: { id: string }) => m.id === id,
      );
      const fields = meta.models.find((m: { name: string }) => m.name === entry).fields;
      const field = (n: string) => fields.find((f: { name: string }) => f.name === n);
      expect(field("note")).toMatchObject({
        kind: "text",
        shows_option_of: `${id}_case.status`,
        shows_option_when: 'kind == "status"',
      });
      expect(field("details")).toMatchObject({
        kind: "longtext",
        shows_option_of: `${id}_case.status`,
        shows_option_when: 'kind == "status"',
      });
      expect(field("role").shows_option_of).toBe(`${id}_case.role`);
      expect(field("role").shows_option_when).toBeUndefined();
      expect(field("kind").shows_option_of).toBeUndefined();

      await signIn("root", `/${id}/${entry}?view=list`);
      await expect(grid(page)).toBeVisible();

      // The status written into a history entry reads as the label of the person's language...
      await expect(line(page, "e1")).toContainText("Pågår");
      await expect(line(page, "e1")).not.toContainText("pagar");
      await expect(line(page, "e4")).toContainText("Klar");
      // ... the role has no condition, so it is relabelled wherever it is an option...
      await expect(line(page, "e1")).toContainText("Ägare");
      await expect(line(page, "e1")).not.toContainText("owner");
      // ... a free note that reads like a key is left alone while the condition is false, ...
      await expect(line(page, "e2")).toContainText("klar");
      await expect(line(page, "e2")).not.toContainText("Klar");
      // ... and a value that is not an option is shown as stored.
      await expect(line(page, "e3")).toContainText("Fri text");
      await expect(line(page, "e3")).toContainText("stranger");
      // The role `helper` has no Swedish label: the generic words, as a select would show.
      await expect(line(page, "e4")).toContainText("Helper");

      // The API still returns what is stored, whatever the language of the request.
      const stored = await api.list("root", entry);
      const byTag = (t: string) => stored.find((r) => r.tag === t);
      expect(byTag("e1")).toMatchObject({ note: "pagar", role: "owner" });
      expect(byTag("e2")).toMatchObject({ note: "klar", kind: "note" });
      expect(byTag("e3")).toMatchObject({ note: "Fri text", role: "stranger" });
      const token = await api.token("root");
      const asked = await api.post(
        `/api/${entry}/search?lang=sv`,
        { filter: [{ field: "note", op: "eq", value: "pagar" }] },
        token,
      );
      expect(asked.json.value.total).toBe(1);
      expect(asked.json.value.rows[0].note).toBe("pagar");
      const byLabel = await api.post(
        `/api/${entry}/search?lang=sv`,
        { filter: [{ field: "note", op: "eq", value: "Pågår" }] },
        token,
      );
      expect(byLabel.json.value.total).toBe(0);
    });

    test("the record panel draws the label for a person who cannot edit, and the stored text for one who can", async ({
      page,
      signIn,
      api,
      scratchPackage,
    }) => {
      const id = await scratchPackage(manifest);
      const entry = `${id}_entry`;
      const made = await api.create("root", entry, lines[0]!);
      const input = (p: Page, f: string) => p.locator(`aside.record #f-${entry}-${f}`);

      // A person who may edit the record edits the stored text, in a text field and in a long text.
      await signIn("root", `/${id}/${entry}?view=list&id=${made.id}`);
      await expect(input(page, "note")).toHaveValue("pagar");
      await expect(input(page, "note")).toBeEditable();
      await expect(input(page, "details")).toHaveValue("pagar");
      await expect(input(page, "details")).toBeEditable();
      // The record is named by the label of its first text field (a header, not an input).
      await expect(page.locator("aside.record h2")).toHaveText("Pågår");
    });

    test("a person who can only read sees the label in the panel", async ({
      page,
      signIn,
      api,
      scratchPackage,
    }) => {
      const id = await scratchPackage(manifest);
      const entry = `${id}_entry`;
      const made = await api.create("root", entry, lines[0]!);
      // `sam` reads through base.user and has no write group of the package.
      await signIn("sam", `/${id}/${entry}?view=list&id=${made.id}`);
      const panel = page.locator("aside.record");
      await expect(panel.locator(`#f-${entry}-note`)).toHaveValue("Pågår");
      await expect(panel.locator(`#f-${entry}-note`)).not.toBeEditable();
      await expect(panel.locator(`#f-${entry}-details`)).toHaveValue("Pågår");
      await expect(panel.locator(`#f-${entry}-details`)).not.toBeEditable();
      await expect(panel.locator(`#f-${entry}-role`)).toHaveValue("Ägare");
      // The stored text is unchanged for the same person through the API.
      const sam = await api.token("sam");
      const got = await api.post(`/api/${entry}/get`, { id: made.id }, sam);
      expect(got.json.value.note).toBe("pagar");
      expect(got.json.value.details).toBe("pagar");
    });
  });

  test("in English it reads as a select would, and the CSV export keeps the stored text", async ({
    page,
    signIn,
    api,
    scratchPackage,
  }) => {
    const id = await scratchPackage(manifest);
    const entry = `${id}_entry`;
    for (const l of lines) await api.create("root", entry, l);
    await signIn("root", `/${id}/${entry}?view=list`);
    await expect(grid(page)).toBeVisible();

    // The package has no English words for the options, so the lookup ends as a select's does:
    // the humanised key.
    await expect(line(page, "e1")).toContainText("Pagar");
    await expect(line(page, "e1")).toContainText("Owner");
    await expect(line(page, "e2")).toContainText("klar");
    await expect(line(page, "e3")).toContainText("Fri text");

    // Export writes the text that is stored, not what is drawn.
    await page.getByRole("checkbox", { name: "Select all records" }).check();
    const download = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export selected" }).click();
    const csv = readFileSync((await (await download).path())!, "utf8");
    const rows = csv.trim().split(/\r?\n/).slice(1);
    const cells = (tag: string) => rows.find((r) => r.split(",").includes(tag))!.split(",");
    expect(cells("e1")).toEqual(expect.arrayContaining(["pagar", "owner"]));
    expect(cells("e2")).toContain("klar");
    expect(cells("e3")).toContain("Fri text");
    expect(csv).not.toMatch(/Pagar|Owner/);
  });
});
