import { describe, expect, test } from "vite-plus/test";
import { LOCALES, messages } from "../../i18n/index.ts";
import { appMirror } from "./app-page.ts";
import {
  FOLD_WORDS,
  LEAD_WORDS,
  appView,
  lowerFirst,
  splitPurpose,
  wordCount,
} from "./app-view.ts";
import { appName, loadApps, type Action, type App, type Field } from "./apps.ts";

const apps = loadApps();

// The words of a text without its punctuation and capitals: what the manifest owner wrote.
const words = (t: string) =>
  t
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();

describe("the helper that compares words really compares them", () => {
  // The helper once lost its backslashes and kept only the letter p, so a purpose that lost words
  // still passed. These cases fail if that happens again.
  test("it keeps letters and digits, and drops the rest", () => {
    expect(words("Prepare customer invoices, as drafts (v2).")).toBe(
      "prepare customer invoices as drafts v2",
    );
    expect(words("Completely different words entirely.")).not.toBe(words("Prepare invoices."));
  });
});

describe("the purpose is split after its first sentence", () => {
  test("every purpose gives a short lead that is one whole sentence and a rest that is the remainder", () => {
    for (const app of apps) {
      const { lead, rest } = splitPurpose(app.purpose);
      expect(lead, app.id).toMatch(/[.!?]$/);
      // The words are the owner's; at most a colon or semicolon at the cut becomes a full stop.
      expect(words(rest === "" ? lead : `${lead} ${rest}`), app.id).toBe(words(app.purpose));
      if (rest) expect(rest, app.id).toMatch(/^[A-Z0-9"(]/);
      // The large lead is short enough to read at a glance; a longer one is a sentence the rule
      // could not cut (no colon or semicolon) and needs a plainer sentence in data/apps.ts.
      expect(wordCount(lead), app.id).toBeLessThanOrEqual(LEAD_WORDS);
    }
  });

  test("only a wall of text folds, and it is the Connectors page today", () => {
    const folded = apps
      .filter((a) => wordCount(splitPurpose(a.purpose).rest) > FOLD_WORDS)
      .map((a) => a.id);
    expect(folded).toEqual(["connectors"]);
    const view = (id: string) =>
      appView(
        apps.find((a) => a.id === id)!,
        messages.en,
        apps,
        "en",
      ).purposeFold;
    expect(view("connectors")).toBe(true);
    expect(view("hr")).toBe(false);
  });

  test("a long first sentence is cut at its colon, which ends in a full stop", () => {
    const long = `${"word ".repeat(30).trim()}: and the rest follows. Second sentence.`;
    const { lead, rest } = splitPurpose(long);
    expect(lead).toBe(`${"word ".repeat(30).trim()}.`);
    expect(rest).toBe("And the rest follows. Second sentence.");
    // A short first sentence keeps its colon.
    expect(splitPurpose("Short: it stays. Next one.").lead).toBe("Short: it stays.");
  });

  test("one sentence has no rest, and an abbreviation does not end a sentence", () => {
    expect(splitPurpose("One sentence only.")).toEqual({ lead: "One sentence only.", rest: "" });
    expect(splitPurpose("It is first. It is second.")).toEqual({
      lead: "It is first.",
      rest: "It is second.",
    });
    expect(splitPurpose("Costs, e.g. tax. More here.").lead).toBe("Costs, e.g. tax.");
  });
});

describe("the view of an app, in every language", () => {
  for (const locale of LOCALES) {
    test(`${locale}: four facts, cards in the page language, a plain compatibility line`, () => {
      const m = messages[locale];
      for (const app of apps) {
        const v = appView(app, m, apps, locale);
        expect(
          v.facts.map((f) => f.label),
          app.id,
        ).toEqual([
          m.apps.meta.maturity,
          m.apps.meta.version,
          m.apps.store.worksWith,
          m.apps.meta.license,
        ]);
        expect(v.category, app.id).toBe(m.apps.categories[app.category]);
        expect(
          v.facts.every((f) => f.value.length > 0),
          app.id,
        ).toBe(true);
        expect(v.actions.length, app.id).toBe(app.models.flatMap((x) => x.actions).length);
        expect(v.models.length, app.id).toBe(app.models.length);
        // Every card is complete, and in a translated language none falls back to English text:
        // the product's label files cover every model and action.
        for (const c of [...v.actions, ...v.models]) {
          expect(c.title.length + c.text.length, app.id).toBeGreaterThan(0);
          expect(c.lang, `${locale} ${app.id} ${c.title}`).toBeUndefined();
        }
        expect(v.info.find((r) => r.label === m.apps.store.compat)?.value, app.id).toBe(
          m.apps.store.compatValue.replace("{version}", "0.2"),
        );
      }
    });
  }

  test("an app has the name the product gives it in the page language, else its English name", () => {
    const byId = (id: string) => apps.find((a) => a.id === id)!;
    const names = (id: string) => ({
      en: appName(byId(id), "en"),
      sv: appName(byId(id), "sv"),
      de: appName(byId(id), "de"),
    });
    expect(names("invoicing")).toEqual({
      en: "Invoicing",
      sv: "Fakturering",
      de: "Rechnungsstellung",
    });
    expect(names("helpdesk").sv).toBe("Kundtjänst");
    // The who-it-works-with and related lists say the same names as the pages themselves.
    const v = appView(byId("field_service"), messages.de, apps, "de");
    expect(v.worksWith.map((d) => d.name)).toContain(appName(byId("base"), "de"));
    expect(v.worksWith.map((d) => d.name)).not.toContain("Contacts");
    // A language without a label file for the module falls back to the English name.
    expect(appName(byId("crm"), "xx")).toBe("CRM");
    // Every app has a name in every language that has a label file, so no page mixes them.
    for (const locale of LOCALES)
      for (const app of apps)
        expect(appName(app, locale).length, `${locale} ${app.id}`).toBeGreaterThan(0);
  });

  test("English cards carry no lang mark and use plain names for the oddly named models", () => {
    const m = messages.en;
    const byId = (id: string) => apps.find((a) => a.id === id)!;
    const titles = (id: string) => appView(byId(id), m, apps, "en").models.map((c) => c.title);
    expect(titles("base")).toEqual(["Contact"]);
    expect(titles("manufacturing")).toEqual(["Bill of materials", "Component", "Order"]);
    expect(titles("accounting")).toContain("Journal entry");
    expect(titles("inventory")).toContain("Stock move");
  });

  test("a language without label files marks every card it takes from the English manifest", () => {
    // 'xx' has no packages/labels/xx: each card falls back to English and says so, for screen readers.
    const v = appView(
      apps.find((a) => a.id === "accounting")!,
      messages.en,
      apps,
      "xx",
    );
    expect(v.actions.length).toBeGreaterThan(0);
    for (const c of [...v.actions, ...v.models]) expect(c.lang, c.title).toBe("en");
    // English itself needs no mark.
    const en = appView(
      apps.find((a) => a.id === "accounting")!,
      messages.en,
      apps,
      "en",
    );
    for (const c of [...en.actions, ...en.models]) expect(c.lang, c.title).toBeUndefined();
  });
});

describe("what it works with is a count, in the plural of the language", () => {
  const some = (n: number) => apps.slice(0, n).map((a) => ({ id: a.id, requirement: "*" }));
  const fact = (locale: (typeof LOCALES)[number], n: number) => {
    const base = apps.find((a) => a.id === "product")!;
    const view = appView({ ...base, dependencies: some(n) }, messages[locale], apps, locale);
    return view.facts.find((f) => f.label === messages[locale].apps.store.worksWith)!.value;
  };

  test("English, Polish and Finnish", () => {
    expect(fact("en", 1)).toBe("1 app");
    expect(fact("en", 2)).toBe("2 apps");
    expect(fact("pl", 1)).toBe("1 aplikacja");
    expect(fact("pl", 2)).toBe("2 aplikacje");
    expect(fact("pl", 5)).toBe("5 aplikacji");
    expect(fact("fi", 1)).toBe("1 sovellus");
    expect(fact("fi", 3)).toBe("3 sovellusta");
  });

  test("no dependency says so in words, and every language has a form for every case", () => {
    for (const locale of LOCALES) {
      const s = messages[locale].apps.store;
      expect(fact(locale, 0), locale).toBe(s.standalone);
      for (const form of Object.values(s.worksCount)) expect(form, locale).toContain("{n}");
      for (const n of [1, 2, 3, 5, 6]) expect(fact(locale, n), `${locale} ${n}`).toMatch(/\d/);
    }
  });
});

describe("a title that starts with an acronym keeps its capitals in the Markdown copy", () => {
  test("lowerFirst", () => {
    expect(lowerFirst("Close")).toBe("close");
    expect(lowerFirst("OAuth start")).toBe("OAuth start");
    expect(lowerFirst("BOM explode")).toBe("BOM explode");
    expect(lowerFirst("A")).toBe("a");
    expect(lowerFirst("")).toBe("");
  });

  test("the Connectors copy says 'OAuth start', as the page does", () => {
    const connectors = apps.find((a) => a.id === "connectors")!;
    const page = appView(connectors, messages.en, apps, "en");
    expect(page.actions.map((c) => c.title)).toContain("OAuth start");
    const md = appMirror(connectors, "en", messages.en, "", apps);
    expect(md).toContain(": OAuth start**");
    expect(md).not.toMatch(/\boAuth\b/);
  });
});

describe("the site's own grouping of the apps is pinned", () => {
  // The category of an app (its quiet line, its row on the index, its related apps) is written by
  // hand in data/apps.ts and is not the manifest's list of categories (shown under For developers,
  // where Helpdesk says "Services"). A change of grouping is a decision: it edits this list too.
  test("the category of every app", () => {
    expect(Object.fromEntries(apps.map((a) => [a.id, a.category]))).toEqual({
      crm: "customers",
      sales: "customers",
      invoicing: "finance",
      product: "foundation",
      base: "foundation",
      connectors: "foundation",
      projects: "people",
      helpdesk: "customers",
      fleet: "operations",
      inventory: "operations",
      purchase: "operations",
      manufacturing: "operations",
      hr: "people",
      timesheets: "people",
      planning: "people",
      field_service: "operations",
      pos: "operations",
      ecommerce: "operations",
      website: "content",
      email_marketing: "content",
      documents: "content",
      sign: "content",
      subscriptions: "customers",
      dashboards: "content",
      accounting: "finance",
      knowledge: "content",
      appointments: "customers",
      expenses: "finance",
    });
  });
});

// Small apps made from a real one, so that one rule of the view is tried at a time.
const crm = apps.find((a) => a.id === "crm")!;
const field = (over: Partial<Field>): Field => ({
  name: "f",
  description: "A field",
  class: "none",
  personal_data: false,
  group: "crm.user",
  kind: "text",
  required: false,
  ...over,
});
const action = (over: Partial<Action>): Action => ({
  name: "close",
  description: "Close it",
  group: "crm.user",
  approval: false,
  ...over,
});
const made = (over: Partial<App>): App => ({ ...crm, ...over });
const view = (app: App, locale: (typeof LOCALES)[number] = "en") =>
  appView(app, messages[locale], apps, locale);

describe("the lines under 'Safe with AI agents' follow the fields and actions of the app", () => {
  const s = messages.en.apps.store;
  const model = (fields: Field[], actions: Action[]) =>
    made({ models: [{ ...crm.models[0]!, fields, actions }] });

  test("personal data is masked, sensitive classes are withheld, and each line has its own words", () => {
    const needsPerson = [action({ approval: true })];
    expect(view(model([field({})], needsPerson)).safe).toEqual([s.safeApprove]);
    expect(view(model([field({ personal_data: true })], needsPerson)).safe).toEqual([
      s.safeMasked,
      s.safeApprove,
    ]);
    expect(view(model([field({ class: "financial" })], needsPerson)).safe).toEqual([
      s.safeWithheld,
      s.safeApprove,
    ]);
    expect(
      view(model([field({ personal_data: true }), field({ class: "secret" })], needsPerson)).safe,
    ).toEqual([s.safeMasked, s.safeWithheld, s.safeApprove]);
    // Personal data of a sensitive class is withheld, not only masked.
    expect(
      view(model([field({ class: "confidential_hr", personal_data: true })], needsPerson)).safe,
    ).toEqual([s.safeWithheld, s.safeApprove]);
    // The two lines are different sentences, in every language.
    for (const locale of LOCALES) {
      const t = messages[locale].apps.store;
      expect(t.safeMasked, locale).not.toBe(t.safeWithheld);
      expect(t.safeApprove, locale).not.toBe(t.safeApproveNone);
    }
  });

  test("the last line says whether a person approves anything", () => {
    expect(view(model([field({})], [action({ approval: true })])).safe).toEqual([s.safeApprove]);
    expect(view(model([field({})], [action({ approval: false })])).safe).toEqual([
      s.safeApproveNone,
    ]);
    expect(view(model([field({})], [])).safe).toEqual([s.safeApproveNone]);
  });
});

describe("the related apps are the others of the same category, six at most", () => {
  const sameCategory = (n: number) =>
    Array.from({ length: n }, (_, i) =>
      made({ id: `x${i}`, slug: `x${i}`, name: `X${i}`, category: "finance" }),
    );

  test("an app never lists itself, and the list stops at six", () => {
    const group = sameCategory(9);
    const v = appView(group[3]!, messages.en, [...group, crm], "en");
    expect(v.related).toHaveLength(6);
    expect(v.related.map((r) => r.id)).not.toContain("x3");
    // The first six others, in the order of the list; another category is left out.
    expect(v.related.map((r) => r.id)).toEqual(["x0", "x1", "x2", "x4", "x5", "x6"]);
    // Fewer than six others: all of them.
    const few = sameCategory(4);
    expect(appView(few[0]!, messages.en, few, "en").related.map((r) => r.id)).toEqual([
      "x1",
      "x2",
      "x3",
    ]);
  });

  test("in the real data: never itself, same category, at most six, and Operations has six", () => {
    for (const app of apps) {
      const v = view(app);
      expect(
        v.related.map((r) => r.id),
        app.id,
      ).not.toContain(app.id);
      expect(v.related.length, app.id).toBeLessThanOrEqual(6);
      for (const r of v.related)
        expect(apps.find((a) => a.id === r.id)!.category, app.id).toBe(app.category);
    }
    expect(view(apps.find((a) => a.id === "fleet")!).related).toHaveLength(6);
  });
});

describe("a long rest of the purpose folds after forty words, and not before", () => {
  const purpose = (restWords: number) =>
    `One short sentence. ${["Word", ...Array(restWords - 1).fill("word")].join(" ")}.`;
  test("forty words stay open, forty-one fold", () => {
    expect(FOLD_WORDS).toBe(40);
    const fold = (n: number) => view(made({ purpose: purpose(n) })).purposeFold;
    expect(fold(1)).toBe(false);
    expect(fold(40)).toBe(false);
    expect(fold(41)).toBe(true);
    expect(fold(120)).toBe(true);
  });
});

describe("the information list", () => {
  const row = (app: App, label: string, locale: (typeof LOCALES)[number] = "en") =>
    view(app, locale).info.find((r) => r.label === label);
  const m = messages.en.apps;
  const builtin = apps.find((a) => a.builtin)!;
  const marketplace = apps.find((a) => !a.builtin)!;
  const licence = (license: string) =>
    row(made({ meta: { ...crm.meta, license } }), m.meta.license)?.href;

  test("the licence links to its SPDX page, unless it is an expression", () => {
    expect(licence("Apache-2.0")).toBe("https://spdx.org/licenses/Apache-2.0.html");
    expect(licence("MIT")).toBe("https://spdx.org/licenses/MIT.html");
    expect(licence("Apache-2.0 OR MIT")).toBeUndefined();
    expect(licence("(MIT)")).toBeUndefined();
  });

  test("a built-in app's version says it moves with obop, a marketplace app's is its own", () => {
    expect(row(builtin, m.meta.version)?.value).toBe(
      m.store.sameAsObop.replace("{version}", builtin.version),
    );
    expect(row(marketplace, m.meta.version)?.value).toBe(marketplace.version);
    // The two are told apart in every language.
    for (const locale of LOCALES)
      expect(row(builtin, messages[locale].apps.meta.version, locale)?.value, locale).not.toBe(
        builtin.version,
      );
  });

  test("the install command is for a marketplace app only, and the source link follows the kind", () => {
    expect(view(marketplace).command).toBe(`obop modules install-from ${marketplace.id}`);
    expect(view(builtin).command).toBeNull();
    expect(row(builtin, m.store.source)?.href).toContain("/tree/main/");
    expect(row(marketplace, m.store.manifest)?.href).toContain("/blob/main/");
  });
});

describe("the title of an action card is its name in words, with a capital", () => {
  test("snake case becomes words, the first letter is a capital, acronyms keep theirs", () => {
    const app = made({
      models: [
        {
          ...crm.models[0]!,
          actions: [
            action({ name: "reset_to_draft" }),
            action({ name: "oauth_start" }),
            action({ name: "bom_explode" }),
            action({ name: "close" }),
          ],
        },
      ],
    });
    expect(view(app).actions.map((c) => c.title)).toEqual([
      "Reset to draft",
      "OAuth start",
      "BOM explode",
      "Close",
    ]);
  });
});

describe("a sentence ends before a capital, a digit, a quote or a bracket", () => {
  test("splitPurpose", () => {
    expect(splitPurpose("First. 2nd follows.")).toEqual({ lead: "First.", rest: "2nd follows." });
    expect(splitPurpose('First. "Quoted" follows.').lead).toBe("First.");
    expect(splitPurpose("First. (Aside) follows.").lead).toBe("First.");
    // A lower-case word after the full stop is the same sentence.
    expect(splitPurpose("It costs 3.5 euro. Next.").lead).toBe("It costs 3.5 euro.");
    expect(splitPurpose("See e.g. this one. Next.").lead).toBe("See e.g. this one.");
  });
});
