import type { Page, Request } from "@playwright/test";
import { expect, open, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

// D70: import records from a CSV file in the list toolbar. The dialog checks the file first (a
// dry run), says which rows are wrong, and writes nothing until the person confirms.
const leadRow = (page: import("@playwright/test").Page, title: string) =>
  page.getByRole("grid", { name: "Lead list" }).getByRole("row", { name: new RegExp(title) });

const csvFile = (body: string) => ({
  name: "leads.csv",
  mimeType: "text/csv",
  buffer: Buffer.from(body),
});

/** Open the Import dialog of the lead list: in the toolbar, or in the ⋯ menu on a phone. */
async function importDialog(page: Page, phone = false) {
  await open(page, "/crm/lead?view=list");
  if (phone) {
    await page.getByRole("button", { name: "More actions" }).click();
    await page.getByRole("menuitem", { name: "Import from a CSV file" }).click();
  } else await page.getByRole("button", { name: "Import", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: /^Import .* from a CSV file$/ });
  await expect(dialog).toBeVisible();
  // The list's own loads are done, so every request counted after this one is the dialog's.
  await page.waitForLoadState("networkidle");
  return dialog;
}

/** The requests a page makes from now on whose address contains `part`. */
function requests(page: Page, part: string): Request[] {
  const seen: Request[] = [];
  page.on("request", (r) => {
    if (r.url().includes(part)) seen.push(r);
  });
  return seen;
}

const csvOf = (r: Request) => (r.postDataJSON() as { csv?: string }).csv;

test.describe("CSV import", () => {
  test("check a file, see what is wrong, import the good rows, and a double click imports once", async ({
    page,
    api,
    signIn,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const a = uid("Import A");
    const b = uid("Import B");
    const c = uid("Import C");
    await signIn("root");
    await open(page, "/crm/lead?view=list");
    await page.getByRole("button", { name: "Import", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: /^Import .* from a CSV file$/ });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText(/Fields you can fill:.*title/)).toBeVisible();
    // The import button is not offered before a file has been checked.
    await expect(dialog.getByRole("button", { name: "Import", exact: true })).toBeDisabled();

    // A file with one bad row (no title, a decimal amount): the dialog names the rows.
    const file = (body: string) => ({
      name: "leads.csv",
      mimeType: "text/csv",
      buffer: Buffer.from(body),
    });
    const bad = `title;contact_name;expected_revenue_minor\r\n${a};Ann Example;125000\r\n;No Title;100\r\n${b};Ben Example;12.50\r\n${c};Cy Example;\r\n`;
    await dialog.getByLabel("Choose a CSV file").setInputFiles(file(bad));
    await expect(dialog.getByText(/Rows ready: 2\. Rows with problems: 2\./)).toBeVisible();
    const problems = dialog.getByRole("table", { name: "Problems found" });
    await expect(problems).toContainText("Title is required");
    await expect(problems).toContainText("without a decimal point");
    await expect(dialog.getByRole("button", { name: "Import", exact: true })).toBeDisabled();
    expect((await api.list("root", "lead")).some((l) => l.title === a)).toBe(false);

    // Skip the bad rows: the good ones import, once, whatever the number of clicks.
    await dialog.getByLabel("If a row is wrong").selectOption("partial");
    const go = dialog.getByRole("button", { name: "Import 2 rows" });
    await expect(go).toBeEnabled();
    await go.dblclick();
    await expect(dialog.getByText(/Created: 2\. Skipped: 2\./)).toBeVisible();
    await dialog.getByRole("button", { name: "Done" }).click();
    await expect(leadRow(page, a)).toContainText("Ann Example");
    await expect(leadRow(page, a)).toContainText("€1,250.00");
    await expect(leadRow(page, c)).toBeVisible();
    await expect(leadRow(page, c)).toContainText("New");
    expect((await api.list("root", "lead")).filter((l) => l.title === a)).toHaveLength(1);

    // The stored rows are the ones the file named; the bad ones are not there.
    expect((await api.list("root", "lead")).some((l) => l.title === b)).toBe(false);
  });

  test("a person who may read but not create sees no Import button", async ({
    page,
    api,
    signIn,
  }) => {
    // sam holds product.user, which reads products; creating them needs product.manager.
    await signIn("sam", "/product/product?view=list");
    await expect(page.getByRole("grid").first()).toBeVisible();
    await expect(page.getByRole("button", { name: "New", exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Import", exact: true })).toHaveCount(0);
    const refused = await api.post(
      "/api/product/bulk_create",
      { rows: [{ name: "x" }] },
      await api.token("sam"),
    );
    expect(refused.status).toBe(404);
    // Where he can create, the button is there.
    await open(page, "/crm/lead?view=list");
    await expect(page.getByRole("button", { name: "Import", exact: true })).toBeVisible();
  });
});

// D70 amendment: a reference column may hold the other record's name, as the lists show it. The
// dialog looks the names up among the records the person may read and sends their ids; a name it
// cannot be sure of stays as written, so the server refuses that row and the dialog says why.
// The server takes 12 bulk calls a minute from one person, dry runs included. This file makes
// 5 as root (3 of them in the first describe), 5 as sam and 4 as dana, so it can run twice in a
// row on one instance (`--repeat-each=2`). A new case here counts its calls against that.
test.describe("CSV import with names in a reference column", () => {
  test("a name in a reference column is looked up and the rows import", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const party = await api.create("root", "party", { name: uid("Import Party") });
    const title = uid("Named lead");
    await signIn("root");
    const dialog = await importDialog(page);
    await expect(
      dialog.getByText(/A column that points to another record \(.*party_id/),
    ).toBeVisible();
    const bulk = requests(page, "/api/lead/bulk_create");
    // Another case and spaces round it: still the same name.
    await dialog
      .getByLabel("Choose a CSV file")
      .setInputFiles(csvFile(`title,party_id\r\n${title},  ${party.name.toUpperCase()} \r\n`));
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 0\./)).toBeVisible();
    expect(bulk).toHaveLength(1);
    expect(csvOf(bulk[0]!)).toBe(`title,party_id\r\n${title},${party.id}\r\n`);
    // Emptied after the pick, so the same file chosen again (once corrected) is read again.
    await expect(dialog.getByLabel("Choose a CSV file")).toHaveValue("");
    await dialog.getByRole("button", { name: /^Import 1 rows?$/ }).click();
    await expect(dialog.getByText(/Created: 1\. Skipped: 0\./)).toBeVisible();
    const made = (await api.list("root", "lead")).filter((l) => l.title === title);
    expect(made.map((l) => l.party_id)).toEqual([party.id]);
  });

  test("a file of ids is sent byte for byte", async ({ page, api, signIn, uid }) => {
    const party = await api.create("root", "party", { name: uid("Id Party") });
    const title = uid("Id lead");
    await signIn("sam");
    const dialog = await importDialog(page);
    const bulk = requests(page, "/api/lead/bulk_create");
    const searches = requests(page, "/api/party/search");
    const body = `title;party_id\r\n"${title}; with a semicolon";${party.id}\r\n\r\n`;
    await dialog.getByLabel("Choose a CSV file").setInputFiles(csvFile(body));
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 0\./)).toBeVisible();
    expect(bulk.map(csvOf)).toEqual([body]);
    expect(searches).toHaveLength(0);
    await dialog.getByRole("button", { name: "Cancel" }).click();
    expect((await api.list("root", "lead")).some((l) => l.title.startsWith(title))).toBe(false);
  });

  test("a name found twice and a name not found are problems on the row and line of the file", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const good = await api.create("root", "party", { name: uid("Good Party") });
    const twin = uid("Twin Party");
    const twins = [
      await api.create("root", "party", { name: twin }),
      await api.create("root", "party", { name: twin }),
    ];
    const missing = uid("Nobody Party");
    const [t1, t2, t3] = [uid("Two-line lead"), uid("Twin lead"), uid("Missing lead")];
    // The first title holds a line break, so every Line after it is one more than its Row.
    const body = `title,party_id\n"${t1}\nsecond line",${good.name}\n${t2},${twin}\n${t3},${missing}\n`;
    await signIn("sam");
    let dialog = await importDialog(page);
    // The good rows import on their own (that Stop keeps Import back is the next case's).
    await dialog.getByLabel("If a row is wrong").selectOption("partial");
    await dialog.getByLabel("Choose a CSV file").setInputFiles(csvFile(body));
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 2\./)).toBeVisible();
    const problems = dialog.getByRole("table", { name: "Problems found" });
    const doubled = problems.getByRole("row").filter({ hasText: "More than one" });
    await expect(doubled.getByRole("cell").nth(0)).toHaveText("2");
    await expect(doubled.getByRole("cell").nth(1)).toHaveText("4");
    await expect(doubled.getByRole("cell").nth(2)).toHaveText(
      `More than one contact is named “${twin}” (column party_id). Give one of them another name, or write the id of the one you mean: open it and copy the text after “id=” from the address bar.`,
    );
    const absent = problems.getByRole("row").filter({ hasText: "No contact" });
    await expect(absent.getByRole("cell").nth(0)).toHaveText("3");
    await expect(absent.getByRole("cell").nth(1)).toHaveText("5");
    await expect(absent.getByRole("cell").nth(2)).toHaveText(
      `No contact is named “${missing}” (column party_id). Check the spelling, or add it first.`,
    );
    await dialog.getByRole("button", { name: /^Import 1 rows?$/ }).click();
    await expect(dialog.getByText(/Created: 1\. Skipped: 2\./)).toBeVisible();
    await dialog.getByRole("button", { name: "Done" }).click();
    let leads = await api.list("root", "lead");
    expect(leads.filter((l) => l.title === `${t1}\nsecond line`).map((l) => l.party_id)).toEqual([
      good.id,
    ]);
    expect(leads.some((l) => l.title === t2 || l.title === t3)).toBe(false);

    // As the sentence says: open one of the two, copy the text after "id=" from the address bar,
    // write it in place of the name, and the row imports.
    await open(page, "/base/party?view=list");
    await page
      .getByRole("grid")
      .getByRole("row", { name: new RegExp(twin) })
      .first()
      .getByRole("gridcell", { name: twin })
      .click();
    await expect(page).toHaveURL(/[?&]id=[0-9a-f-]{36}/);
    const id = /[?&]id=([0-9a-f-]{36})/.exec(page.url())![1]!;
    expect(twins.map((p) => p.id)).toContain(id);
    dialog = await importDialog(page);
    await dialog
      .getByLabel("Choose a CSV file")
      .setInputFiles(csvFile(`title,party_id\n${t2},${id}\n`));
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 0\./)).toBeVisible();
    await dialog.getByRole("button", { name: /^Import 1 rows?$/ }).click();
    await expect(dialog.getByText(/Created: 1\. Skipped: 0\./)).toBeVisible();
    leads = await api.list("root", "lead");
    expect(leads.filter((l) => l.title === t2).map((l) => l.party_id)).toEqual([id]);
  });

  test("the check and the import send the same text and look the names up once", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const party = await api.create("root", "party", { name: uid("Once Party") });
    const missing = uid("Nobody Party");
    const [t1, t2] = [uid("Once lead"), uid("Once missing lead")];
    await signIn("dana");
    const dialog = await importDialog(page);
    const bulk = requests(page, "/api/lead/bulk_create");
    const searches = requests(page, "/api/party/search");
    await dialog
      .getByLabel("Choose a CSV file")
      .setInputFiles(csvFile(`title,party_id\n${t1},${party.name}\n${t2},${missing}\n`));
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 1\./)).toBeVisible();
    // "Stop: import nothing" offers no import while a row has a problem.
    await expect(dialog.getByRole("button", { name: "Import", exact: true })).toBeDisabled();
    await dialog.getByLabel("If a row is wrong").selectOption("partial");
    await expect(dialog.getByRole("button", { name: /^Import 1 rows?$/ })).toBeEnabled();
    await dialog.getByRole("button", { name: /^Import 1 rows?$/ }).click();
    await expect(dialog.getByText(/Created: 1\. Skipped: 1\./)).toBeVisible();
    const sent = `title,party_id\n${t1},${party.id}\n${t2},${missing}\n`;
    expect(bulk.map(csvOf)).toEqual([sent, sent, sent]);
    const keys = bulk.map(
      (r) => (r.postDataJSON() as { idempotency_key?: string }).idempotency_key,
    );
    expect(keys.slice(0, 2)).toEqual([undefined, undefined]);
    expect(keys[2]).toMatch(/^[0-9a-f-]{36}$/);
    expect(searches).toHaveLength(1);
    const leads = await api.list("root", "lead");
    expect(leads.filter((l) => l.title === t1).map((l) => l.party_id)).toEqual([party.id]);
    expect(leads.some((l) => l.title === t2)).toBe(false);
  });

  test("at 390 px the dialog with a found, a missing and a doubled name fits", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const found = await api.create("root", "party", { name: uid("Phone Party") });
    const twin = uid("Phone Twin");
    await api.create("root", "party", { name: twin });
    await api.create("root", "party", { name: twin });
    const missing = uid("Phone Nobody");
    await page.setViewportSize({ width: 390, height: 844 });
    await signIn("dana");
    const dialog = await importDialog(page, true);
    await dialog
      .getByLabel("Choose a CSV file")
      .setInputFiles(
        csvFile(
          `title,party_id\n${uid("Phone lead")},${found.name}\n${uid("Phone lead")},${missing}\n${uid("Phone lead")},${twin}\n`,
        ),
      );
    await expect(dialog.getByText(/Rows ready: 1\. Rows with problems: 2\./)).toBeVisible();
    await expect(dialog.getByText(/No contact is named/)).toBeVisible();
    await expect(dialog.getByText(/More than one contact is named/)).toBeVisible();
    // Below the hints, the report is brought into view: its first problem is whole on screen.
    const first = dialog
      .getByRole("table", { name: "Problems found" })
      .getByRole("row")
      .filter({ hasText: "No contact is named" });
    await expect(first).toBeInViewport({ ratio: 1 });
    expect(await layoutProblems(page)).toEqual([]);
    const sizes = await dialog.evaluate((root) => {
      const box = root.querySelector<HTMLElement>(".dialog-body") ?? root;
      // A word that a line ends in the middle of has its parts on two lines.
      const broken: string[] = [];
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        const el = n.parentElement;
        if (!el || el.closest(".sr-only, option, select")) continue;
        let clipped = false;
        for (let e: Element | null = el; e; e = e.parentElement)
          if (getComputedStyle(e).clipPath === "inset(50%)") clipped = true;
        if (clipped) continue;
        const text = n.textContent ?? "";
        for (const m of text.matchAll(/[^\s\-/]+/g)) {
          const range = document.createRange();
          range.setStart(n, m.index!);
          range.setEnd(n, m.index! + m[0].length);
          const tops = new Set(
            [...range.getClientRects()].filter((r) => r.width > 0).map((r) => Math.round(r.top)),
          );
          if (tops.size > 1) broken.push(m[0]);
        }
      }
      const small = [...root.querySelectorAll<HTMLElement>("button")]
        .filter((b) => b.getBoundingClientRect().height > 0)
        .filter((b) => b.getBoundingClientRect().height < 44)
        .map(
          (b) =>
            `${(b.textContent || b.getAttribute("aria-label") || "").trim()}: ${b.getBoundingClientRect().height}`,
        );
      return {
        sideways: [root, box].filter((b) => b.scrollWidth > b.clientWidth + 1).length,
        broken,
        small,
      };
    });
    expect(sizes).toEqual({ sideways: 0, broken: [], small: [] });
  });
});
