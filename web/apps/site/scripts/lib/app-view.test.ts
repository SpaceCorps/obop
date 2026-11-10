import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test, vi } from "vite-plus/test";
import { LOCALES, messages } from "../../i18n/index.ts";
import { appMirror, developerBody } from "./app-page.ts";
import {
  FOLD_WORDS,
  LEAD_WORDS,
  appView,
  capital,
  lowerFirst,
  modelLabel,
  splitPurpose,
  splitRest,
  words as wordsOf,
  wordCount,
} from "./app-view.ts";
import {
  BUILTIN_MATURITY,
  appName,
  loadApps,
  type Action,
  type App,
  type Field,
  type Labels,
} from "./apps.ts";
import { OVERSTATED } from "./claims.ts";
import { TECHNICAL, technicalIn, visible } from "./plain.ts";

// Label files that exist only in this test, for an app id no real app has: the rules that decide
// whether a card speaks the page language (all of its words are there) are tried one word at a time.
const fake = vi.hoisted(() => ({ labels: {} as Record<string, Labels> }));
vi.mock("./apps.ts", async (importOriginal) => {
  const real = await importOriginal<typeof import("./apps.ts")>();
  return {
    ...real,
    loadLabels: (locale: string, id: string) => fake.labels[id] ?? real.loadLabels(locale, id),
  };
});

const apps = loadApps();
const REPO = join(import.meta.dirname, "../../../../..");

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
      // One sentence, counted and not only cut: the splitter ends a sentence before a capital, so a
      // sentence that begins with the product's lower-case name ("obop does not ...") would stay in
      // the large type behind the first one (Email Marketing's did).
      expect(lead.match(/[.!?](?=\s|$)/g), `${app.id} lead: ${lead}`).toHaveLength(1);
      // So no sentence of a purpose begins in lower case ("e.g." and "i.e." are not ends).
      expect(
        app.purpose.replace(/\b(?:e\.g|i\.e)\./g, "").match(/[.!?]\s+[a-z]/g) ?? [],
        `${app.id}: a sentence that begins in lower case`,
      ).toEqual([]);
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

  test("a long rest keeps its first sentence in view and folds the others", () => {
    const connectors = appView(
      apps.find((a) => a.id === "connectors")!,
      messages.en,
      apps,
      "en",
    );
    // The caveat of the page is the first sentence of the rest, so a reader sees it without opening
    // anything: what the controls are, and that they are not legal advice or a certification.
    expect(connectors.purposeRest).toBe(
      "These are technical controls that support GDPR compliance, not legal advice or a certification.",
    );
    expect(connectors.purposeMore).toContain("Article 30");
    expect(connectors.purposeMore).not.toContain("not legal advice");
    // Nothing is lost: the Markdown copy says all of it, in order.
    const md = appMirror(
      apps.find((a) => a.id === "connectors")!,
      "en",
      messages.en,
      "",
      apps,
    );
    expect(md).toContain(
      `${connectors.purposeLead} ${connectors.purposeRest} ${connectors.purposeMore}`,
    );
  });

  test("splitRest: forty words stay shown, a longer rest folds after its first sentence", () => {
    const sentence = (n: number) => `${["Word", ...Array(n - 1).fill("word")].join(" ")}.`;
    expect(splitRest("")).toEqual({ shown: "", more: "" });
    expect(splitRest(sentence(40))).toEqual({ shown: sentence(40), more: "" });
    // 41 words in one sentence: nothing short enough to show alone, so all of it folds.
    expect(splitRest(sentence(41))).toEqual({ shown: "", more: sentence(41) });
    // Two sentences, 41 words in all: the first is shown, the second folds.
    expect(splitRest(`${sentence(10)} ${sentence(31)}`)).toEqual({
      shown: sentence(10),
      more: sentence(31),
    });
    // A first sentence that is itself longer than 40 words is not shown on its own.
    expect(splitRest(`${sentence(45)} Short.`)).toEqual({
      shown: "",
      more: `${sentence(45)} Short.`,
    });
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
        // Every card is complete, and in a translated language none falls back to English text
        // except a card that data/apps.ts gives plain words of its own (English, and marked): the
        // product's label files cover every other model and action.
        const written = new Set(Object.keys(app.cards));
        const keys = [
          ...app.models.flatMap((model) => model.actions.map((x) => `${model.name}.${x.name}`)),
          ...app.models.map((model) => model.name),
        ];
        [...v.actions, ...v.models].forEach((c, i) => {
          expect(c.title.length + c.text.length, app.id).toBeGreaterThan(0);
          expect(c.lang, `${locale} ${app.id} ${c.title}`).toBe(
            locale !== "en" && written.has(keys[i]!) ? "en" : undefined,
          );
        });
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

describe("a model has its name without the app's prefix, and a name that has no prefix is kept whole", () => {
  test("the cards of the apps whose model names carry no app prefix", () => {
    // sale_order in Sales, lead in CRM, invoice in Invoicing and product in Products start with no
    // "<app>_": cutting the length of the prefix off the front would read "Rder", "", "Oice".
    const titles = (id: string) => view(apps.find((a) => a.id === id)!).models.map((c) => c.title);
    expect(titles("sales")).toEqual(["Sale order", "Sale order line"]);
    expect(titles("crm")).toEqual(["Lead"]);
    expect(titles("invoicing")).toEqual(["Invoice", "Invoice line"]);
    expect(titles("product")).toEqual(["Product"]);
  });

  test("a prefix is cut once, and only where it is there", () => {
    const helpdesk = apps.find((a) => a.id === "helpdesk")!;
    const model = (name: string) => ({ ...helpdesk.models[0]!, name });
    expect(modelLabel(helpdesk, model("helpdesk_ticket"))).toBe("Ticket");
    expect(modelLabel(helpdesk, model("ticket"))).toBe("Ticket");
    expect(modelLabel(helpdesk, model("sale_order"))).toBe("Sale order");
  });
});

describe("the acronyms of the manifests keep their capitals in a title", () => {
  test("words() and capital()", () => {
    expect(capital(wordsOf("export_csv"))).toBe("Export CSV");
    expect(wordsOf("helpdesk_sla")).toBe("helpdesk SLA");
    expect(wordsOf("oauth_start")).toBe("OAuth start");
    expect(wordsOf("bom_line")).toBe("BOM line");
    expect(wordsOf("reset_to_draft")).toBe("reset to draft");
  });

  test("the real cards: Export CSV (Connectors) and SLA (Helpdesk)", () => {
    const titles = (id: string) => view(apps.find((a) => a.id === id)!).actions.map((c) => c.title);
    expect(titles("connectors")).toContain("Export CSV");
    expect(titles("connectors")).not.toContain("Export csv");
    // The Helpdesk card says "Response times" (data/apps.ts); without that name the model reads "SLA".
    const helpdesk = apps.find((a) => a.id === "helpdesk")!;
    expect(view(helpdesk).models.map((c) => c.title)).toContain("Response times");
    const sla = helpdesk.models.find((x) => x.name === "helpdesk_sla")!;
    expect(modelLabel({ ...helpdesk, modelNames: {} }, sla)).toBe("SLA");
  });
});

describe("the install command names the module, whose id is not always its page address", () => {
  test("an id with an underscore is written as the module's id, not as the slug", () => {
    for (const [id, slug] of [
      ["field_service", "field-service"],
      ["email_marketing", "email-marketing"],
    ] as const) {
      const app = apps.find((a) => a.id === id)!;
      expect(app.slug).toBe(slug);
      expect(view(app).command).toBe(`obop modules install-from ${id}`);
    }
    // Every marketplace app: its id, whatever its slug.
    for (const app of apps.filter((a) => !a.builtin))
      expect(view(app).command, app.id).toBe(`obop modules install-from ${app.id}`);
  });
});

// The name that the German label file gives a model (the product's own word for it), read from the
// TOML file itself and not through the site's loader.
function germanModelName(id: string, model: string): string | undefined {
  const toml = readFileSync(join(REPO, "packages/labels/de", `${id}.toml`), "utf8");
  const head = `[labels.de.model.${model}]`;
  const at = toml.indexOf(head);
  if (at < 0) return undefined;
  return /^name\s*=\s*"([^"]+)"/m.exec(toml.slice(at + head.length))?.[1];
}

describe("a card on a translated page uses the page language's own words, all or none", () => {
  const thing = {
    ...crm.models[0]!,
    name: "thing",
    actions: [action({ name: "close", description: "Close it" })],
  };
  const app = made({ id: "fake_labels", slug: "fake-labels", models: [thing] });
  const cards = (labels: Labels, locale: (typeof LOCALES)[number] = "de") => {
    fake.labels["fake_labels"] = labels;
    const v = view(app, locale);
    return { action: v.actions[0]!, model: v.models[0]! };
  };
  const full: Labels = {
    models: { thing: { name: "Ding", description: "Ein Ding" } },
    actions: { "thing.close": { label: "Schliessen", description: "Schliesst es" } },
  };

  test("with every word there, the page language is used and nothing is marked English", () => {
    const { action: a, model: m } = cards(full);
    expect(a).toMatchObject({ kicker: "Ding", title: "Schliessen", text: "Schliesst es" });
    expect(a.lang).toBeUndefined();
    expect(m).toMatchObject({ title: "Ding", text: "Ein Ding" });
    expect(m.lang).toBeUndefined();
  });

  test("an action card with the model's name, the label or the description missing is English, and says so", () => {
    for (const labels of [
      { ...full, models: { thing: { description: "Ein Ding" } } },
      { ...full, actions: { "thing.close": { description: "Schliesst es" } } },
      { ...full, actions: { "thing.close": { label: "Schliessen" } } },
    ] as Labels[]) {
      const a = cards(labels).action;
      expect(a, JSON.stringify(labels)).toMatchObject({
        kicker: "Thing",
        title: "Close",
        text: "Close it",
        lang: "en",
      });
    }
  });

  test("a model card with the name or the description missing is English, and says so", () => {
    for (const labels of [
      { ...full, models: { thing: { description: "Ein Ding" } } },
      { ...full, models: { thing: { name: "Ding" } } },
    ] as Labels[]) {
      const m = cards(labels).model;
      expect(m, JSON.stringify(labels)).toMatchObject({
        title: "Thing",
        text: thing.description,
        lang: "en",
      });
    }
  });

  test("English never reads a label file", () => {
    const { action: a, model: m } = cards(full, "en");
    expect(a).toMatchObject({ kicker: "Thing", title: "Close", text: "Close it" });
    expect(a.lang).toBeUndefined();
    expect(m.title).toBe("Thing");
  });

  test("in the real data every action card of a translated page has the label file's name for its model", () => {
    delete fake.labels["fake_labels"];
    let differs = 0;
    for (const real of apps) {
      const v = view(real, "de");
      const owners = real.models.flatMap((model) => model.actions.map(() => model));
      expect(v.actions.length, real.id).toBe(owners.length);
      v.actions.forEach((card, i) => {
        const own = germanModelName(real.id, owners[i]!.name);
        expect(own, `${real.id} ${owners[i]!.name} has a German name`).toBeTruthy();
        // A card with plain words of its own is English on a translated page (and says so).
        if (card.lang === "en") return;
        expect(card.kicker, `${real.id} ${card.title}`).toBe(own);
        if (own !== modelLabel(real, owners[i]!)) differs++;
      });
    }
    // Not vacuous: the German names are not the English ones.
    expect(differs).toBeGreaterThan(10);
  });
});

describe("the large first sentence is cut only when it is longer than 25 words", () => {
  const sentence = (n: number) => `${["Start:", ...Array(n - 1).fill("word")].join(" ")}.`;
  test("25 words keep their colon, 26 are cut at it", () => {
    expect(LEAD_WORDS).toBe(25);
    const whole = splitPurpose(`${sentence(25)} The rest.`);
    expect(wordCount(whole.lead)).toBe(25);
    expect(whole.lead).toBe(sentence(25));
    expect(whole.rest).toBe("The rest.");
    const cut = splitPurpose(`${sentence(26)} The rest.`);
    expect(cut.lead).toBe("Start.");
    expect(cut.rest).toMatch(/^Word word.* The rest\.$/);
  });
});

describe("the plain text of an app page leaves the technical words to the part for developers", () => {
  // The large sentence is for a person who does not build software, and so is everything else
  // that is on the first two screens: the paragraph under it, the cards that are shown without
  // opening a fold, the lines about AI agents, the strip, the line under it.

  test("no large first sentence and no quiet paragraph uses one", () => {
    for (const app of apps) {
      const { lead, rest } = splitPurpose(app.purpose);
      expect(technicalIn(lead), `${app.id} lead`).toBeNull();
      // The rest may say "pseudonym" where it explains it in the same sentence (Connectors).
      expect(technicalIn(rest), `${app.id} rest`).toBeNull();
    }
  });

  test("nor does a card that is shown without opening a fold, a line about AI agents, or the strip", () => {
    for (const app of apps) {
      const v = view(app);
      for (const card of [...visible(v.actions), ...visible(v.models)])
        expect(
          technicalIn(`${card.kicker ?? ""} ${card.title} ${card.text}`),
          `${app.id} card "${card.title}": ${card.text}`,
        ).toBeNull();
      for (const line of v.safe) expect(technicalIn(line), `${app.id}: ${line}`).toBeNull();
      for (const f of v.facts) expect(technicalIn(f.value), `${app.id} ${f.label}`).toBeNull();
      expect(technicalIn(messages.en.apps.taglines[app.id] ?? ""), `${app.id} tagline`).toBeNull();
    }
    expect(technicalIn(messages.en.apps.store.experimentalNote)).toBeNull();
    expect(technicalIn(messages.en.apps.store.directEdit)).toBeNull();
  });

  test("the rule can fail: every word it lists is found by its own example, and the list cannot shrink unseen", () => {
    // The words themselves are pinned here, so deleting one from plain.ts (the list could once lose
    // seventeen of its twenty-three without a test noticing) fails this line, and adding one needs a
    // row with an example in plain.ts and a line here.
    expect(TECHNICAL.map((t) => t.word)).toEqual([
      "API",
      "SKU",
      "KPI",
      "hash(?:es)?",
      "SHA-?\\d*",
      "payload",
      "immutable",
      "withheld",
      "allow-?list",
      "pseudonyms?",
      "approval-gated",
      "anonymi[sz]e[sd]?",
      "masked",
      "clients?",
      "schema",
      "quer(?:y|ies)",
      "JSON",
      "endpoint",
      "data class(?:es)?",
      "widgets?",
      "aggregate",
      "SLA",
      "approvers?",
    ]);
    for (const { word, example } of TECHNICAL) {
      // The example is found by this word and by no other: the rule built without the word does not
      // see it, so the word is what the example tests.
      expect(new RegExp(`\\b(?:${word})\\b`, "i").test(example), `${word}: ${example}`).toBe(true);
      const others = TECHNICAL.filter((t) => t.word !== word).map((t) => t.word);
      expect(
        new RegExp(`\\b(?:${others.join("|")})\\b`, "i").test(example),
        `${word}: ${example} must not be found by another word`,
      ).toBe(false);
      expect(technicalIn(example), example).not.toBeNull();
    }
    // A word is a whole word: a longer word that merely contains one is not found.
    expect(technicalIn("a classic clientele with an apiary")).toBeNull();
    for (const bad of ["record signed_by first", "needs the inventory.user group"])
      expect(technicalIn(bad), bad).not.toBeNull();
    for (const good of [
      "Keep track of possible sales, from the first contact until you win or lose them.",
      "Personal details go out under a code name (a pseudonym) unless you decide otherwise.",
      "only people who may approve",
    ])
      expect(technicalIn(good), good).toBeNull();
    // The cards a fold hides are not part of the rule: seven cards are all shown, eight fold after six.
    expect(visible([1, 2, 3, 4, 5, 6, 7])).toHaveLength(7);
    expect(visible([1, 2, 3, 4, 5, 6, 7, 8])).toHaveLength(6);
  });

  test("what was left out is in the technical part, in the Markdown copy", () => {
    const withTechnical = apps.filter((a) => a.technical);
    expect(withTechnical.length).toBeGreaterThan(0);
    for (const app of withTechnical) {
      expect(app.purpose, app.id).not.toContain(app.technical);
      const md = appMirror(app, "en", messages.en, "", apps);
      const cut = md.indexOf(`\n## ${messages.en.apps.store.developers}\n`);
      expect(md.slice(cut), app.id).toContain(app.technical!);
      expect(md.slice(0, cut), app.id).not.toContain(app.technical!);
    }
  });
});

describe("an experimental app says in one plain line that it is still being tested", () => {
  test("the flag follows the maturity, and the Markdown copy has the line once, after the facts", () => {
    const withMaturity = (maturity: string) => made({ meta: { ...crm.meta, maturity } });
    expect(view(withMaturity("experimental")).experimental).toBe(true);
    expect(view(withMaturity("beta")).experimental).toBe(false);
    expect(view(withMaturity("stable")).experimental).toBe(false);
    for (const locale of LOCALES) {
      const s = messages[locale].apps.store;
      const md = appMirror(withMaturity("experimental"), locale, messages[locale], "", apps);
      expect(md.split(s.experimentalNote).length - 1, locale).toBe(1);
      expect(md.indexOf(s.experimentalNote), locale).toBeGreaterThan(md.indexOf("- **"));
      expect(md.indexOf(s.experimentalNote), locale).toBeLessThan(md.indexOf("\n## "));
      expect(
        appMirror(withMaturity("beta"), locale, messages[locale], "", apps),
        locale,
      ).not.toContain(s.experimentalNote);
    }
    // Real data: the flag is the manifest's own word, and some app is experimental.
    const real = apps.filter((a) => a.meta.maturity === "experimental");
    expect(real.length).toBeGreaterThan(0);
    for (const app of apps) expect(view(app).experimental, app.id).toBe(real.includes(app));
  });
});

describe("the mark that a person approves a step is a few plain words, in every language", () => {
  test("short, and without the word agent", () => {
    // An approval is asked for when an AI agent asks: a person who holds the permission runs the step
    // directly and nobody approves it, so the words say whose step it is (the AI's) and not only that
    // a person approves.
    const ai = {
      en: /\bAI\b/,
      sv: /\bAI\b/,
      de: /\bKI\b/,
      fr: /\bIA\b/,
      pl: /\bAI\b/,
      fi: /^Tekoäly/,
      es: /\bIA\b/,
    };
    for (const locale of LOCALES) {
      const text = messages[locale].apps.store.needsApproval;
      expect(text.length, locale).toBeLessThanOrEqual(40);
      expect(text, locale).not.toMatch(/agent/i);
      expect(text, locale).toMatch(ai[locale as keyof typeof ai]);
    }
    // Every card that needs a person has the mark, in the Markdown copy too.
    const accounting = apps.find((a) => a.id === "accounting")!;
    const md = appMirror(accounting, "en", messages.en, "", apps);
    const marked = view(accounting).actions.filter((c) => c.approval).length;
    expect(marked).toBeGreaterThan(1);
    expect(md.split(`(${messages.en.apps.store.needsApproval})`).length - 1).toBe(marked);
  });
});

describe("a card with plain words of its own (data/apps.ts)", () => {
  const withCards = apps.filter((a) => Object.keys(a.cards).length > 0);

  test("every key is a real action (model.action) or model, so a renamed one cannot go stale", () => {
    expect(withCards.length).toBeGreaterThan(8);
    for (const app of withCards)
      for (const key of Object.keys(app.cards)) {
        const real =
          app.models.some((model) => model.name === key) ||
          app.models.some((model) => model.actions.some((x) => `${model.name}.${x.name}` === key));
        expect(real, `${app.id}: ${key}`).toBe(true);
      }
  });

  test("the English card says the written words, keeps the approval mark and the model it works on", () => {
    const accounting = apps.find((a) => a.id === "accounting")!;
    const post = view(accounting).actions.find((c) => c.title === "Post")!;
    expect(post.text).toBe(accounting.cards["accounting_move.post"]!.text);
    expect(post.kicker).toBe("Journal entry");
    expect(post.approval).toBe(true);
    expect(post.lang).toBeUndefined();
    // A card that has none keeps the manifest's words.
    const cancel = view(accounting).actions.find((c) => c.title === "Cancel")!;
    expect(cancel.text).toBe("Cancel a draft entry that was never posted");
    // A model card too, and a written title where the words of a name read oddly.
    const dashboards = apps.find((a) => a.id === "dashboards")!;
    expect(view(dashboards).models.map((c) => c.title)).toEqual(["Dashboard", "Dashboard item"]);
    expect(view(dashboards).models[0]!.text).toBe(dashboards.cards["dashboards_dashboard"]!.text);
  });

  test("on a translated page it is English and marked, because the translation says the old words", () => {
    const sign = apps.find((a) => a.id === "sign")!;
    for (const locale of LOCALES.filter((l) => l !== "en")) {
      const cards = view(sign, locale).actions;
      const send = cards[0]!;
      expect(send.text, locale).toBe(sign.cards["sign_request.send"]!.text);
      expect(send.lang, locale).toBe("en");
      expect(send.kicker, locale).toBe("Signature request");
      // The cards without plain words of their own still speak the page language.
      const decline = cards.find(
        (c) => c.text !== sign.cards["sign_request.send"]!.text && !c.lang,
      );
      expect(decline, locale).toBeDefined();
    }
  });

  test("what the overstating cards said is gone, and what replaced it says less", () => {
    const text = (id: string, key: string) => apps.find((a) => a.id === id)!.cards[key]!.text;
    // Nothing in the repository sends an email: the cards say that obop records the step and does
    // not deliver the message.
    expect(text("email_marketing", "email_marketing_campaign.send")).toContain(
      "obop does not send the emails itself",
    );
    expect(text("email_marketing", "email_marketing_campaign.send_now")).toContain(
      "obop does not send the emails itself",
    );
    expect(text("sign", "sign_request.send")).toContain("does not email the signer");
    expect(text("purchase", "purchase_order.send")).toContain("does not contact the vendor");
    expect(text("subscriptions", "subscriptions_subscription.activate")).toContain(
      "does not start any billing",
    );
    expect(text("accounting", "accounting_period.close")).toContain("label only");
    expect(text("accounting", "accounting_fiscal_year.close")).toContain("label only");
    for (const key of ["ecommerce_order.refund", "ecommerce_order.refund_delivered"])
      expect(text("ecommerce", key), key).toContain("you pay it back yourself");
    expect(text("pos", "pos_order.refund")).toContain("does not pay money back");
  });

  // A ratchet over the pages as they are drawn, not over the cards alone (claims.ts: each sentence
  // was on a page, and the module does not do what it said). The browser tests run the same list over
  // the pages as deployed.
  test("no page of any app says what the module does not do again", () => {
    for (const app of apps) {
      const page = appMirror(app, "en", messages.en, "", apps);
      for (const [bad, why] of OVERSTATED)
        expect(page, `${app.id} ${bad}: ${why}`).not.toMatch(bad);
    }
  });

  test("the ratchet can fail: each sentence is caught where it was, in a card, the lead or the technical part", () => {
    const invoicing = apps.find((a) => a.id === "invoicing")!;
    // The technical part: the model text, with the old claim put back, is found.
    const old: App = {
      ...invoicing,
      models: invoicing.models.map((m) =>
        m.name === "invoice"
          ? {
              ...m,
              description: "A customer invoice; posted invoices are corrected only by credit notes",
            }
          : m,
      ),
    };
    expect(appMirror(old, "en", messages.en, "", apps)).toMatch(OVERSTATED[0]![0]);
    expect(appMirror(invoicing, "en", messages.en, "", apps)).not.toMatch(OVERSTATED[0]![0]);
    // The package text of Email Marketing is the site's, and keeps the manifest's other sentences.
    const email = apps.find((a) => a.id === "email_marketing")!;
    const manifest = readFileSync(join(REPO, "packages/email_marketing.toml"), "utf8");
    const own = /^description = "([^"]+)"/m.exec(manifest)![1]!;
    expect(own).toMatch(/irreversible and reaches real people/);
    expect(email.description).not.toMatch(/irreversible and reaches real people/);
    for (const kept of [
      "Campaign statistics are numbers entered or imported, not computed.",
      "never mail a contact that is not opted_in.",
    ])
      expect(
        [own, email.description].every((t) => t.includes(kept)),
        kept,
      ).toBe(true);
  });
});

describe("the page and its Markdown copy hold the same technical tables", () => {
  const tokens = (md: string) =>
    md.replace(/<code>(.*?)<\/code>/g, (_m, t: string) => `\`${t.replace(/<wbr>/g, "")}\``);

  test("the copy has no <wbr> and no <code> tag, in any language, and the page's tokens are the copy's", () => {
    for (const app of apps)
      for (const locale of LOCALES) {
        const m = messages[locale];
        const copy = appMirror(app, locale, m, "", apps);
        expect(copy, `${locale} ${app.id}: a break point in the Markdown copy`).not.toContain(
          "<wbr>",
        );
        expect(copy, `${locale} ${app.id}: an HTML token in the Markdown copy`).not.toContain(
          "<code>",
        );
      }
    // The tables of the page are the copy's, token for token (English: nothing else differs).
    for (const app of apps) {
      const page = developerBody(app, "en", messages.en, false, "", apps);
      const copy = developerBody(app, "en", messages.en, true, "", apps);
      expect(page, app.id).toContain("<wbr>");
      expect(tokens(page), app.id).toBe(copy);
      // Each token of a table cell, with the break points taken out, is the plain token.
      for (const cell of page.match(/<code>.*?<\/code>/g) ?? [])
        expect(copy, `${app.id} ${cell}`).toContain(
          `\`${cell.replace(/<\/?code>/g, "").replace(/<wbr>/g, "")}\``,
        );
    }
  });
});

describe("the plain layer says AI assistants, and the technical part says agents", () => {
  test("no word 'agent' in the heading, the lines or the large text, in any language", () => {
    const agent = /agent|agenci|agentit/i;
    for (const locale of LOCALES) {
      const s = messages[locale].apps.store;
      for (const key of [
        "safe",
        "safeMasked",
        "safeWithheld",
        "safeApprove",
        "safeApproveNone",
      ] as const)
        expect(s[key], `${locale} ${key}`).not.toMatch(agent);
    }
    for (const app of apps) {
      const { lead, rest } = splitPurpose(app.purpose);
      expect(`${lead} ${rest}`, app.id).not.toMatch(agent);
    }
  });

  test("the line about what is withheld names the kinds of detail, in every language", () => {
    // It said only "sensitive details": a visitor could not learn that an AI assistant cannot read a
    // journal entry's amounts. The kinds are those of the classes the product withholds (financial,
    // confidential_hr, special_category, secret).
    const kinds: Record<string, RegExp[]> = {
      en: [/money/, /bank/, /pay/, /health/, /sign-in/],
      de: [/Geldbeträge/, /Bankdaten/, /Gehälter/, /Gesundheit/, /Zugangsdaten/],
      es: [/importes/, /bancarios/, /salarios/, /salud/, /credenciales/],
      fi: [/rahamääri/, /pankki/, /palkk/, /terveys/, /kirjautumis/],
      fr: [/montants/, /bancaires/, /salaires/, /santé/, /identifiants/],
      pl: [/kwoty/, /bankowe/, /wynagrodzenia/, /zdrowiu/, /logowania/],
      sv: [/belopp/, /bank/, /löner/, /hälso/, /inloggning/],
    };
    for (const locale of LOCALES)
      for (const word of kinds[locale]!)
        expect(messages[locale].apps.store.safeWithheld, locale).toMatch(word);
  });
});

describe("the maturity the site states", () => {
  test("the six built-in apps are rated beta, as the FAQ says obop is; a marketplace app keeps its manifest's word", () => {
    expect(BUILTIN_MATURITY).toBe("beta");
    const builtin = apps.filter((a) => a.builtin);
    expect(builtin).toHaveLength(6);
    for (const app of builtin) {
      expect(app.meta.maturity, app.id).toBe("beta");
      expect(view(app).facts[0]!.value, app.id).toBe(messages.en.apps.meta.maturities.beta);
      expect(view(app).experimental, app.id).toBe(false);
    }
    const sign = apps.find((a) => a.id === "sign")!;
    expect(sign.meta.maturity).toBe("experimental");
    expect(view(sign).facts[0]!.value).toBe(messages.en.apps.meta.maturities.experimental);
  });
});
