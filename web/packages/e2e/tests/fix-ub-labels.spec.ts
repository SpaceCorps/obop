import { expect, test, type Api } from "../support/fixtures.ts";

// Package labels that read well in every language (docs/ISSUES.md G9 and Q18): the card summary of
// Accounting Sweden is translated like every other module's, and the German name of the invoices
// list fits the title of the list at 320 px instead of being cut to "Ausgang...".

const LANGS = ["de", "fr", "pl", "fi", "sv", "es"] as const;

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const installed = (await api.run("root", "module.list")).map((m: any) => m.id);
  const added: string[] = [];
  for (const id of ids)
    if (!installed.includes(id)) {
      await api.run("root", "marketplace_listing.install", { id });
      added.push(id);
    }
  try {
    await run();
  } finally {
    for (const id of added.reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

test("Accounting Sweden has its card summary in the six languages (G9)", async ({ api }) => {
  await withPackages(api, ["accounting", "accounting_se"], async () => {
    const root = await api.token("root");
    const meta = (await api.get("/api/meta", root)).json.modules;
    const se = meta.find((m: any) => m.id === "accounting_se");
    const english = se.summary ?? meta.find((m: any) => m.id === "accounting")?.summary;
    for (const lang of LANGS) {
      const text = se.summaries?.[lang];
      expect(typeof text === "string" && text.trim().length > 0, lang).toBe(true);
      expect(text, lang).not.toBe(english);
    }
  });
});

test.describe("the German invoices list at 320 px (Q18)", () => {
  test.use({
    language: "de",
    viewport: { width: 320, height: 700 },
    hasTouch: true,
    isMobile: true,
  });

  test("the list title is not cut", async ({ page, signIn, api }) => {
    const root = await api.token("root");
    const meta = (await api.get("/api/meta", root)).json.modules;
    const invoice = meta.flatMap((m: any) => m.models ?? []).find((m: any) => m.name === "invoice");
    const plural: string = invoice.plural_labels?.de;
    expect(plural, "the German plural of invoice").toBeTruthy();
    await signIn("root", "/invoicing/invoice");
    // The title: the visible element that holds exactly the plural.
    const title = page.locator(".crumbs strong", { hasText: plural }).filter({
      visible: true,
    });
    await expect(title.first()).toBeVisible();
    const cut = await title
      .first()
      .evaluate((e) => e.scrollWidth > e.clientWidth + 1 || e.scrollHeight > e.clientHeight + 1);
    expect(cut, `"${plural}" is cut at 320 px`).toBe(false);
  });
});
