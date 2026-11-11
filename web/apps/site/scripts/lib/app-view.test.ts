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
  limitsOf,
  lowerFirst,
  modelLabel,
  splitPurpose,
  splitRest,
  words as wordsOf,
  wordCount,
} from "./app-view.ts";
import {
  agentView as agentViewOf,
  appName,
  loadApps,
  loadLabels,
  type Action,
  type App,
  type Field,
  type Labels,
} from "./apps.ts";
import { CLAIMS } from "./claims.ts";
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

/**
 * The line about what is withheld, as it must read for these data classes (in the order of the table).
 * `moneyInClear`: the app has a price or a total an AI agent can read, and the line says so after the list.
 */
const withheld = (
  locale: (typeof LOCALES)[number],
  classes: ("financial" | "confidential_hr" | "special_category" | "secret")[],
  moneyInClear = false,
) => {
  const t = messages[locale].apps.store;
  const kinds = classes.map((c) => t.safeKinds[c]);
  const line = t.safeWithheld.replace(
    "{list}",
    new Intl.ListFormat(locale, { style: "long", type: "conjunction" }).format(kinds),
  );
  return moneyInClear ? `${line} ${t.safeMoneyClear}` : line;
};
/** An app has a money field that an AI agent reads in clear (its data class is none, and it is not personal data). */
const moneyInClear = (app: App) =>
  app.models.some((m) => m.fields.some((f) => f.kind === "money" && agentViewOf(f) === "clear"));

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
      withheld("en", ["financial"]),
      s.safeApprove,
    ]);
    expect(
      view(model([field({ personal_data: true }), field({ class: "secret" })], needsPerson)).safe,
    ).toEqual([s.safeMasked, withheld("en", ["secret"]), s.safeApprove]);
    // Personal data of a sensitive class is withheld, not only masked.
    expect(
      view(model([field({ class: "confidential_hr", personal_data: true })], needsPerson)).safe,
    ).toEqual([withheld("en", ["confidential_hr"]), s.safeApprove]);
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
      expect(technicalIn(v.safeWhat), `${app.id}: ${v.safeWhat}`).toBeNull();
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
    // "By itself": a webhook connection can post the record on these commands, and the DocuSeal
    // package sends the request on sign_request.send (claims.ts), so a flat denial says more than the
    // module does.
    expect(text("sign", "sign_request.send")).toContain(
      "By itself, this does not email the signer",
    );
    expect(text("purchase", "purchase_order.send")).toContain(
      "By itself, this does not contact the vendor",
    );
    expect(text("purchase", "purchase_order.confirm")).toContain(
      "By itself, this does not contact the vendor",
    );
    expect(text("subscriptions", "subscriptions_subscription.activate")).toContain(
      "By itself, this does not start any billing",
    );
    expect(text("accounting", "accounting_period.close")).toContain("label only");
    expect(text("accounting", "accounting_fiscal_year.close")).toContain("label only");
    for (const key of ["ecommerce_order.refund", "ecommerce_order.refund_delivered"])
      expect(text("ecommerce", key), key).toContain("you pay it back yourself");
    expect(text("pos", "pos_order.refund")).toContain("By itself, this does not pay money back");
    // refund runs only from paid (packages/ecommerce.toml:665-672): a shipped order has no refund.
    expect(text("ecommerce", "ecommerce_order.refund")).toMatch(
      /^Record the refund of an order that is paid and not yet shipped\./,
    );
    // The point of sale has no rules: its two flags are read by nothing (packages/pos.toml:44-60).
    expect(text("pos", "pos_config")).not.toMatch(/\brules?\b/i);
    // The tax rates are read by the public checkout only (crates/kernel/obop-site/src/checkout.rs:142-170).
    expect(text("ecommerce", "ecommerce_tax_rate")).toContain("public checkout");
  });

  test("the technical text and the corrections of the package text say only what the packages do", () => {
    const app = (id: string) => apps.find((a) => a.id === id)!;
    // Nothing supports a custom storefront on the API (docs/STOREFRONT.md:127; docs/PRODUCT.md, Open).
    expect(app("ecommerce").technical).toBeUndefined();
    // The checkout takes stock only for these two policies (crates/kernel/obop-site/src/checkout.rs:318-322).
    expect(app("ecommerce").description).toContain(
      "whose stock policy is block_when_out or allow_backorder",
    );
    expect(app("ecommerce").description).not.toMatch(/Orders take stock off/);
    // The stock moves of a till sale need a stock location, and the package needs Inventory (packages/pos.toml:16-19, :232).
    expect(app("pos").description).toContain(
      "When the sale has a stock location, paying also creates",
    );
    expect(app("pos").description).toContain("Needs the Products and Inventory packages.");
    // Publishing is for knowledge.manager (packages/knowledge.toml:80-102), and nothing per article is promised.
    expect(app("knowledge").purpose).toContain("managers publish, unpublish and archive them");
    expect(app("knowledge").purpose).not.toMatch(/\byet\b|can write, arrange in a tree, publish/);
  });

  // A ratchet over the pages as they are drawn, not over the cards alone (claims.ts: each sentence
  // was on a page, and the module does not do what it said). The browser tests run the same list over
  // the pages as deployed. It keeps these wordings from coming back; it does not prove that the pages
  // are true (the manifest facts a card may assert are reviewed by a person).
  test("the wordings on the list of false sentences are on no page of any app and not on the index", () => {
    for (const app of apps) {
      const page = appMirror(app, "en", messages.en, "", apps);
      for (const { re, why } of CLAIMS) expect(page, `${app.id} ${re}: ${why}`).not.toMatch(re);
    }
    for (const { re, why } of CLAIMS)
      expect(messages.en.apps.lead, `the apps index ${re}: ${why}`).not.toMatch(re);
  });

  // Every row of the list can fail: it catches the sentence it was written for, and leaves alone
  // the true sentences that stand where it was (a pattern that is deleted, weakened or made so broad
  // that it catches its own correction fails here). The count is pinned, so a row is not dropped
  // unnoticed.
  test("each row of the ratchet catches its own sentence and leaves the corrections alone", () => {
    expect(CLAIMS).toHaveLength(60);
    for (const { re, example, fine, why } of CLAIMS) {
      expect(example, `${re}: ${why}`).toMatch(re);
      for (const ok of fine ?? []) expect(ok, `${re} must leave this alone`).not.toMatch(re);
    }
    // The wider rows catch other wordings of the same claim.
    const claim = (text: string) => CLAIMS.filter((c) => c.re.test(text)).length;
    for (const text of [
      "Closing a period locks it against new entries.",
      "Closing the year blocks further posting.",
      "Closing a month stops posting.",
      "Publishing needs approval.",
      "No step in this app needs an approval, apart from deleting records.",
    ])
      expect(claim(text), text).toBeGreaterThan(0);
    for (const text of [
      "Closing a month or year is only a label; it does not stop new entries.",
      "Closing a period never blocks entries.",
      "No step in this app needs approval when a person runs it.",
    ])
      expect(claim(text), text).toBe(0);
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
    expect(appMirror(old, "en", messages.en, "", apps)).toMatch(CLAIMS[0]!.re);
    expect(appMirror(invoicing, "en", messages.en, "", apps)).not.toMatch(CLAIMS[0]!.re);
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

describe("the plain layer says AI agents and explains the word once; the technical part says agents", () => {
  // The word each language uses for an AI agent on its home page and in its docs.
  const AGENT: Record<(typeof LOCALES)[number], RegExp> = {
    en: /\bAI agents?\b/,
    sv: /\bAI-agent(?:er)?\b/,
    de: /\bKI-Agent(?:en)?\b/,
    fr: /\bagents? IA\b/,
    pl: /\bagen(?:t|ci) AI\b/i,
    fi: /\btekoälyagentti|\btekoälyagentit/i,
    es: /\bagentes? de IA\b/,
  };
  // The word the owner dropped, in each language (the plural of Polish "asystent" is "asystenci"). Spanish
  // "asistente" alone is also an attendee, as the product's own label of an appointment says, so only
  // "asistente de IA" is the old word.
  const ASSISTANT = /assistants?\b|assistent|asystent|asystenc|avustaj|asistentes? de IA/i;

  test("the rule that finds the old word can fail, in every language", () => {
    for (const example of [
      "AI assistants and this app",
      "KI-Assistenten und diese App",
      "Les assistants IA",
      "Asystenci AI i ta aplikacja",
      "Tekoälyavustajat näkevät",
      "Los asistentes de IA",
      "AI-assistenter och den här appen",
    ])
      expect(example).toMatch(ASSISTANT);
    expect("Tekoälyagentit ja tämä sovellus").not.toMatch(ASSISTANT);
    for (const locale of LOCALES) expect(messages[locale].apps.store.safe).toMatch(AGENT[locale]);
  });

  test("the heading and the lines say 'AI agent' in the language's own word, and none says assistant", () => {
    for (const locale of LOCALES) {
      const s = messages[locale].apps.store;
      for (const key of [
        "safe",
        "safeWhat",
        "safeMasked",
        "safeWithheld",
        "safeApprove",
        "safeApproveNone",
      ] as const) {
        expect(s[key], `${locale} ${key}`).toMatch(AGENT[locale]);
        expect(s[key], `${locale} ${key}`).not.toMatch(ASSISTANT);
      }
      for (const kind of Object.values(s.safeKinds)) expect(kind, locale).not.toMatch(ASSISTANT);
      expect(s.needsApproval, locale).not.toMatch(ASSISTANT);
      expect(messages[locale].apps.lead, locale).not.toMatch(ASSISTANT);
    }
    // The sentence, as the owner asked for it.
    expect(messages.en.apps.store.safeWhat).toBe(
      "An AI agent is an AI program that works in obop for a person, and can do no more than that person may.",
    );
  });

  test("every app page has the explanation exactly once in its plain part, in every language, and no assistant", () => {
    for (const locale of LOCALES) {
      const m = messages[locale];
      const s = m.apps.store;
      for (const app of apps) {
        const where = `${locale} ${app.id}`;
        expect(appView(app, m, apps, locale).safeWhat, where).toBe(s.safeWhat);
        const md = appMirror(app, locale, m, "", apps);
        const cut = md.indexOf(`## ${s.developers}`);
        expect(cut, where).toBeGreaterThan(0);
        const plain = md.slice(0, cut);
        // Once, and right under the heading that it opens, before the lines.
        expect(plain.split(s.safeWhat).length - 1, where).toBe(1);
        expect(plain, where).toContain(`## ${s.safe}\n\n${s.safeWhat}\n\n- `);
        expect(plain, where).not.toMatch(ASSISTANT);
        // And not again in the part for developers.
        expect(md.split(s.safeWhat).length - 1, where).toBe(1);
      }
    }
  });

  test("the part for developers keeps the word 'agent'", () => {
    for (const locale of LOCALES) {
      const a = messages[locale].apps;
      for (const app of apps) {
        const body = developerBody(app, locale, messages[locale], true, "", apps);
        expect(body, `${locale} ${app.id}`).toContain(a.agentsPropose.split("{list}")[0]!);
        expect(body, `${locale} ${app.id}`).not.toMatch(ASSISTANT);
      }
    }
  });

  // A visitor could not learn what an AI agent cannot read when the line said only "sensitive
  // details"; but a line that names money amounts is false where the amounts are in clear (an order
  // total, a vehicle's cost: their data class is none). So the line names the kinds of the data
  // classes that are withheld on this page, and no others.
  test("the line about what is withheld names only the kinds this app withholds, in every language", () => {
    const classes = ["financial", "confidential_hr", "special_category", "secret"] as const;
    for (const locale of LOCALES) {
      const t = messages[locale].apps.store;
      expect(t.safeWithheld, locale).toContain("{list}");
      expect(
        new Set(classes.map((c) => t.safeKinds[c])).size,
        `${locale}: four different kinds`,
      ).toBe(4);
    }
    for (const app of apps) {
      const held = new Set(
        app.models.flatMap((m) =>
          m.fields.filter((f) => agentViewOf(f) === "withheld").map((f) => f.class),
        ),
      );
      for (const locale of LOCALES) {
        const t = messages[locale].apps.store;
        const line = view(app, locale).safe.find((l) =>
          l.startsWith(t.safeWithheld.split("{list}")[0]!),
        );
        if (held.size === 0) {
          expect(
            line,
            `${app.id} ${locale}: a line about withheld details on an app that withholds none`,
          ).toBeUndefined();
          continue;
        }
        expect(line, `${app.id} ${locale}`).toBe(
          withheld(
            locale,
            classes.filter((c) => held.has(c)),
            moneyInClear(app),
          ),
        );
        for (const c of classes)
          expect(line!.includes(t.safeKinds[c]), `${app.id} ${locale}: ${c}`).toBe(held.has(c));
      }
    }
  });

  test("it does not name money where the amounts are in clear: Fleet, Purchase, Point of Sale and eCommerce show what they withhold", () => {
    const line = (id: string) =>
      view(apps.find((a) => a.id === id)!).safe.find((l) => /never see/.test(l))!;
    // The line says what is held back, and where an amount is in clear, that a price or a total is
    // shown unless its own field is one of these.
    const never = "AI agents never see the fields marked as sensitive. In this app:";
    const tail = " A price or a total is shown unless its field is one of these.";
    expect(line("fleet")).toBe(`${never} confidential HR details.${tail}`);
    expect(line("purchase")).toBe(`${never} sensitive financial details.${tail}`);
    expect(line("pos")).toContain(tail);
    expect(line("ecommerce")).toBe(
      `${never} sensitive financial details and secrets (such as sign-in details).${tail}`,
    );
    // And where there is no such amount the sentence is not there: nothing on the page is a price.
    expect(line("connectors")).toBe(`${never} secrets (such as sign-in details).`);
    expect(line("hr")).toBe(
      `${never} sensitive financial details, confidential HR details, and special-category personal data (such as health).`,
    );
    // The amounts of these four apps are in clear, as their own tables say, so no line may say
    // that money, bank details or pay are hidden from an AI agent for them.
    for (const id of ["fleet", "purchase", "pos", "ecommerce"]) {
      const app = apps.find((a) => a.id === id)!;
      const money = app.models
        .flatMap((m) => m.fields.map((f) => ({ ...f, model: m.name })))
        .filter((f) => f.kind === "money" && agentViewOf(f) === "clear");
      expect(money.length, `${id}: money fields in clear`).toBeGreaterThan(0);
      expect(view(app).safe.join(" "), id).not.toMatch(/money|amount|bank|pay\b/i);
    }
  });

  // The sentence "a price or a total is shown" was the tail of every withheld line, also on pages with
  // no price or total an AI agent can read (Accounting, Expenses, Timesheets, Contacts, HR, Connectors).
  // Now only an app that has such an amount says it, and this list is the whole set: a new money field
  // in clear, or a new sensitive field next to one, changes it and the test says so.
  test("the sentence that a price or a total is shown is on exactly four pages, in every language", () => {
    const withTail = ["ecommerce", "fleet", "pos", "purchase"];
    for (const locale of LOCALES) {
      const t = messages[locale].apps.store;
      expect(t.safeMoneyClear.length, locale).toBeGreaterThan(10);
      expect(t.safeMoneyClear, locale).not.toMatch(ASSISTANT);
      // The line it follows must not already contain it, and the key is not part of that line.
      expect(t.safeWithheld, locale).not.toContain(t.safeMoneyClear);
      const got = apps
        .filter((app) =>
          view(app, locale).safe.some(
            (l) => l.startsWith(t.safeWithheld.split("{list}")[0]!) && l.endsWith(t.safeMoneyClear),
          ),
        )
        .map((app) => app.id)
        .sort();
      expect(got, locale).toEqual(withTail);
      // It is the last words of the withheld line, and on no other line.
      for (const app of apps)
        expect(
          view(app, locale).safe.filter((l) => l.includes(t.safeMoneyClear)).length,
          `${app.id} ${locale}`,
        ).toBe(withTail.includes(app.id) ? 1 : 0);
    }
    // The four are the apps that withhold something and also show an amount in clear.
    expect(
      apps
        .filter(
          (app) =>
            moneyInClear(app) &&
            app.models.some((m) => m.fields.some((f) => agentViewOf(f) === "withheld")),
        )
        .map((app) => app.id)
        .sort(),
    ).toEqual(withTail);
  });

  test("the tail follows the fields: money in clear next to a withheld field, and only then", () => {
    const needsPerson = [action({ approval: true })];
    const s = messages.en.apps.store;
    const model = (fields: Field[], actions: Action[]) =>
      made({ models: [{ ...crm.models[0]!, fields, actions }] });
    const safe = (fields: Field[]) => view(model(fields, needsPerson)).safe;
    // A price in clear and a withheld field: the line has the tail.
    expect(safe([field({ kind: "money" }), field({ class: "financial" })])[0]).toBe(
      withheld("en", ["financial"], true),
    );
    // A withheld money field and nothing in clear: no tail.
    expect(safe([field({ kind: "money", class: "financial" })])[0]).toBe(
      withheld("en", ["financial"]),
    );
    // Not a money kind, even if named like one: no tail.
    expect(safe([field({ name: "total", kind: "number" }), field({ class: "secret" })])[0]).toBe(
      withheld("en", ["secret"]),
    );
    // Money in clear and nothing withheld: no line about what is withheld, so no tail either.
    expect(safe([field({ kind: "money" })])).toEqual([s.safeApprove]);
    // A money field that is personal data is masked, not in clear: no tail.
    expect(
      safe([field({ kind: "money", personal_data: true }), field({ class: "secret" })]),
    ).toEqual([s.safeMasked, withheld("en", ["secret"]), s.safeApprove]);
  });

  test("the developer part lists a delete step only for a model that has one", () => {
    const connectors = apps.find((a) => a.id === "connectors")!;
    const propose = (app: App) =>
      /can only propose these steps[\s\S]*?\n\n/.exec(
        developerBody(app, "en", messages.en, true, "", apps),
      )?.[0] ?? "";
    // Connectors: create, update and delete exist for three models; the log, the runs, the register
    // and the other read-only models cannot be deleted, and a consent is kept as evidence.
    const list = propose(connectors);
    expect(list).toContain("`connection.delete`");
    expect(list).toContain("`provider_policy.delete`");
    for (const model of connectors.noDelete)
      expect(list, model).not.toContain(`\`${model}.delete\``);
    expect(connectors.noDelete).toContain("data_flow_log");
    expect(connectors.noDelete).toContain("consent");
    // Sign: a signing link is kept as a record, so even a person's approval cannot make an AI agent
    // delete one (crates/kernel/obop-commands/src/sign/link.rs:30-40); a request can still be deleted.
    const sign = propose(apps.find((a) => a.id === "sign")!);
    expect(sign).not.toContain("`sign_link.delete`");
    expect(sign).toContain("`sign_request.delete`");
    // An app that did not say so lists every model.
    const crmApp = apps.find((a) => a.id === "crm")!;
    expect(propose(crmApp)).toContain("`lead.delete`");
  });

  test("a field description the module words too strongly is corrected in the table, not the others", () => {
    const invoicing = apps.find((a) => a.id === "invoicing")!;
    const number = invoicing.models
      .find((m) => m.name === "invoice")!
      .fields.find((f) => f.name === "number")!;
    expect(number.description).toBe(
      "Gapless yearly number such as INV-2026-00042, assigned when the invoice is posted",
    );
    const total = invoicing.models
      .find((m) => m.name === "invoice")!
      .fields.find((f) => f.name === "total_minor")!;
    expect(total.description).toBe("Invoice total in minor units, the sum of the lines");
  });
});

describe("the maturity the site states", () => {
  const maturityOf = (file: string) =>
    /^maturity\s*=\s*"([^"]+)"/m.exec(readFileSync(join(REPO, file), "utf8"))![1]!;

  test("the six built-in apps show the maturity their snapshot and the Rust code say, which is stable; nothing overrides it", () => {
    const builtin = apps.filter((a) => a.builtin);
    expect(builtin.map((a) => a.id).sort()).toEqual([
      "base",
      "connectors",
      "crm",
      "invoicing",
      "product",
      "sales",
    ]);
    // What the Rust says: every built-in module takes its metadata from `ModuleMeta::first_party`,
    // which rates it stable, and none of them changes the word afterwards.
    const registry = readFileSync(join(REPO, "crates/kernel/obop-registry/src/lib.rs"), "utf8");
    expect(registry).toContain('maturity: "stable".into()');
    const module: Record<string, string> = {
      base: "crates/modules/base/src/lib.rs",
      product: "crates/modules/product/src/lib.rs",
      crm: "crates/modules/crm/src/lib.rs",
      sales: "crates/modules/sales/src/lib.rs",
      invoicing: "crates/modules/invoicing/src/lib.rs",
      connectors: "crates/modules/connectors/src/meta.rs",
    };
    for (const app of builtin) {
      const code = readFileSync(join(REPO, module[app.id]!), "utf8");
      expect(code, `${app.id}: takes its metadata from first_party`).toContain(
        "ModuleMeta::first_party(",
      );
      expect(code, `${app.id}: sets no maturity of its own`).not.toMatch(/\.maturity\s*=/);
      // The snapshot is a copy of that metadata, and the page shows the snapshot's word.
      const snapshot = maturityOf(`web/apps/site/data/builtin/${app.id}.toml`);
      expect(snapshot, app.id).toBe("stable");
      expect(app.meta.maturity, app.id).toBe(snapshot);
      expect(view(app).facts[0]!.value, app.id).toBe(messages.en.apps.meta.maturities.stable);
      expect(view(app).experimental, app.id).toBe(false);
    }
  });

  test("a marketplace app keeps its manifest's word", () => {
    const marketplace = apps.filter((a) => !a.builtin);
    expect(marketplace).toHaveLength(22);
    for (const app of marketplace)
      expect(app.meta.maturity, app.id).toBe(maturityOf(`packages/${app.id}.toml`));
    const sign = apps.find((a) => a.id === "sign")!;
    expect(sign.meta.maturity).toBe("experimental");
    expect(view(sign).facts[0]!.value).toBe(messages.en.apps.meta.maturities.experimental);
  });

  test("no built-in app is called beta on its page or in its Markdown copy, in any language", () => {
    for (const locale of LOCALES) {
      const m = messages[locale];
      // The locale's word for beta as a whole word ("Betalning" is not it).
      const word = new RegExp(`(?<!\\p{L})${m.apps.meta.maturities.beta}(?!\\p{L})`, "iu");
      const stable = m.apps.meta.maturities.stable;
      for (const app of apps.filter((a) => a.builtin)) {
        const v = appView(app, m, apps, locale);
        expect(v.facts[0]!.value, `${locale} ${app.id}`).toBe(stable);
        expect(v.info.find((r) => r.label === m.apps.meta.maturity)!.value).toBe(stable);
        const md = appMirror(app, locale, m, "", apps);
        expect(md, `${locale} ${app.id}`).not.toMatch(word);
        // The strip, the information list and the technical table all say it, and say the same.
        expect(
          md.split(`**${m.apps.meta.maturity}:** ${stable}`).length - 1,
          `${locale} ${app.id}`,
        ).toBe(3);
      }
      // The apps index no longer calls the apps beta (the marketplace ones keep their own word).
      expect(m.apps.lead, locale).not.toMatch(word);
    }
  });
});

describe("a caveat that only a developer needs sits under 'For developers', moved and not copied", () => {
  const en = messages.en;
  const cut = (md: string) => md.indexOf(`\n## ${en.apps.store.developers}\n`);
  /** Every caveat that data/apps.ts moves: a card's `limit` and an app's `limits`. */
  const moved = apps.flatMap((app) => [
    ...Object.entries(app.cards)
      .filter(([, c]) => c.limit !== undefined)
      .map(([key, c]) => ({ app, key, text: c.text, limit: c.limit! })),
    ...app.limits.map((limit) => ({ app, key: "(app)", text: app.purpose, limit })),
  ]);
  const strip = (html: string) => html.replace(/<wbr>/g, "").replace(/<\/?code>/g, "");

  test("these are the caveats that moved; a new one is a change of this list", () => {
    expect(moved.map((x) => `${x.app.id} ${x.key}`).sort()).toEqual(
      [
        "accounting accounting_move.post",
        "base party.erase",
        "ecommerce ecommerce_order.refund",
        "ecommerce ecommerce_order.refund_delivered",
        "fleet fleet_vehicle_driver",
        "planning (app)",
        "pos pos_order.pay",
        "pos pos_session.close",
      ].sort(),
    );
  });

  test("a limit is one sentence a Markdown list can hold, and the text it leaves is a sentence of its own", () => {
    for (const { app, key, text, limit } of moved) {
      const where = `${app.id} ${key}`;
      expect(limit.trim(), where).toBe(limit);
      expect(limit, where).toMatch(/[.]$/);
      expect(limit, where).not.toMatch(/[<>*|]/);
      expect(limit, where).not.toMatch(/^[-#]/);
      expect(limit.split("\n").length, where).toBe(1);
      // One sentence: a full stop is followed by no further sentence (a semicolon joins two halves).
      expect(limit.split(/(?<=[.])\s+/), where).toHaveLength(1);
      expect(wordCount(text), `${where}: the text on top`).toBeGreaterThanOrEqual(3);
      expect(text.includes(limit), `${where}: the limit is not a copy of the text`).toBe(false);
      expect(app.purpose.includes(limit), where).toBe(false);
    }
  });

  test("the limit is in the part for developers of the Markdown copy, under the package heading, and not above it", () => {
    for (const { app, key, limit } of moved) {
      const md = appMirror(app, "en", en, "", apps);
      const at = cut(md);
      expect(at, `${app.id} ${key}`).toBeGreaterThan(0);
      expect(md.slice(0, at), `${app.id} ${key}: above the fold`).not.toContain(limit);
      // Two cards may share one sentence (the two refunds of eCommerce): once for each.
      const sharing = moved.filter((x) => x.app === app && x.limit === limit).length;
      expect(md.slice(at).split(limit).length - 1, `${app.id} ${key}`).toBe(sharing);
      // And under the heading of the package, before the next one.
      const heading = md.indexOf(`### ${en.apps.h.package}`, at);
      const next = md.indexOf(`\n### `, heading + 1);
      expect(md.indexOf(limit, at), `${app.id} ${key}`).toBeGreaterThan(heading);
      expect(md.indexOf(limit, at), `${app.id} ${key}`).toBeLessThan(next);
    }
  });

  test("the plain words that stay on top are still true on their own, and say what the limit does not", () => {
    const card = (id: string, key: string) => apps.find((a) => a.id === id)!.cards[key]!;
    const eco = card("ecommerce", "ecommerce_order.refund");
    for (const key of ["ecommerce_order.refund", "ecommerce_order.refund_delivered"]) {
      const c = card("ecommerce", key);
      expect(c.text, key).toContain("the order stays paid if Stripe refuses. Otherwise you pay it");
      expect(c.text, key).not.toMatch(/stand-in|tested/);
      expect(c.limit, key).toBe(eco.limit);
    }
    expect(eco.limit).toContain("stand-in");
    expect(card("pos", "pos_order.pay").text).toContain(
      "a paid sale can still be edited afterwards",
    );
    expect(card("pos", "pos_order.pay").text).not.toContain("press pay");
    expect(card("pos", "pos_order.pay").limit).toBe(
      "The payments are checked only when the sale is paid.",
    );
    expect(card("pos", "pos_order.pay").limit).not.toMatch(/press/i);
    expect(card("pos", "pos_session.close").text).toBe("Close the session (manager only).");
    // cash_control is a flag nothing reads (packages/pos.toml:44-48, and the field text below it says so),
    // so the package text does not call a session "cash-controlled" (it is shown in the fold, on the
    // marketplace page and in llms-full.txt).
    expect(apps.find((a) => a.id === "pos")!.description).not.toMatch(/cash-controlled/i);
    // The module guide on /docs/reference/module-manifest/ is docs/MODULES.md as it stands, so its
    // `pos` rows (the module list and section 12) must not say it either.
    const posRows = readFileSync(join(REPO, "docs/MODULES.md"), "utf8")
      .split("\n")
      .filter((line) => line.startsWith("| `pos` |"));
    expect(posRows.length).toBeGreaterThan(0);
    for (const row of posRows) expect(row).not.toMatch(/cash-controlled/i);
    expect(card("pos", "pos_session.close").limit).toBe(
      "obop does not check that the drawer was counted.",
    );
    // Nothing reads a driver's state or licence expiry (packages/fleet.toml:63-68, :77-82, :134-154): the
    // suspend card says so on top, and the model card's limit adds the licence expiry, in the fold.
    expect(card("fleet", "fleet_vehicle_driver.suspend").text).toContain(
      "By itself, this does not stop the driver being assigned a vehicle.",
    );
    expect(card("fleet", "fleet_vehicle_driver").text).not.toMatch(/authori[sz]ed|checked/i);
    expect(card("fleet", "fleet_vehicle_driver").limit).toBe(
      "A driver's status and licence expiry are not checked when a vehicle is assigned.",
    );
    expect(card("fleet", "fleet_vehicle_driver.reactivate").text).not.toMatch(/allow|drive again/i);
    expect(card("accounting", "accounting_move.post").text).toContain(
      "A posted entry can still be edited afterwards.",
    );
    expect(card("accounting", "accounting_move.post").text).not.toContain("no lines");
    // Said once, in the fold: the sum of no lines is zero on both sides, so the check "debits equal
    // credits" is met (packages/accounting.toml:321, :346; computed.rs, the empty child set sums to 0).
    expect(card("accounting", "accounting_move.post").limit).toBe(
      "An entry with no lines has debits and credits of zero, which are equal, so it can be posted.",
    );
    // The sign-off reminder is said once, by the field text in the part for developers, so the card has no limit.
    const complete = card("field_service", "field_service_job.complete");
    expect(complete.text).not.toContain("Nothing checks");
    expect(complete.limit).toBeUndefined();
    const job = apps
      .find((a) => a.id === "field_service")!
      .models.find((m) => m.name === "field_service_job")!;
    expect(job.fields.find((f) => f.name === "signature_required")!.description).toContain(
      "a job can be finished without it",
    );
    // The contact's erase keeps the day on top: an AI agent can go on using the old name for a while,
    // and for an agent that reaches obop through the obop mcp command until that command stops (a
    // reader of "a local session" would take their own self-hosted install for it).
    const erase = card("base", "party.erase");
    expect(erase.text).toContain("up to a day");
    expect(erase.text).toContain(
      "through the obop mcp command, the placeholder keeps standing until that command stops.",
    );
    expect(erase.text).not.toMatch(/local session|list of placeholders/);
    expect(erase.limit).toMatch(/24 hours[\s\S]*2 hours[\s\S]*command stops[\s\S]*obop mcp\.$/);
    expect(erase.limit).not.toMatch(/local/);
  });

  test("Dashboards and Planning: the sentence is gone from the top, in every place the top says it", () => {
    const dashboards = apps.find((a) => a.id === "dashboards")!;
    expect(dashboards.purpose).not.toMatch(/does not check|points to/);
    // The part for developers had it already (the technical text), so nothing is added there.
    expect(dashboards.technical).toContain("does not check that the models a widget names exist");
    expect(dashboards.limits).toEqual([]);
    const planning = apps.find((a) => a.id === "planning")!;
    for (const text of [
      planning.purpose,
      planning.description,
      planning.models.find((x) => x.name === "planning_template")!.description,
    ])
      expect(text).not.toMatch(/copy into shifts|to copy into/);
    expect(planning.limits).toEqual([
      "No action creates a shift from a template, and a shift has no field that names one.",
    ]);
    expect(appMirror(planning, "en", en, "", apps)).toContain(`- ${planning.limits[0]}`);
    // The top does not say a template makes shifts: nothing in the plain text says "reusable" or "copy".
    for (const text of [
      planning.purpose,
      planning.description,
      planning.models.find((x) => x.name === "planning_template")!.description,
      planning.cards["planning_template"]!.text,
    ])
      expect(text, "the plain text of the template").not.toMatch(/reusable|copy|copied/i);
  });

  test("Planning: the Template card says the same thing in every language and never that a template is copied into shifts", () => {
    const planning = apps.find((a) => a.id === "planning")!;
    const card = planning.cards["planning_template"]!;
    expect(card.text).toBe(
      "A shift pattern, with a name, start and end times and, optionally, a role.",
    );
    for (const locale of LOCALES) {
      const v = view(planning, locale);
      const shown = v.models.find(
        (c) =>
          c.title ===
          modelLabel(
            planning,
            planning.models.find((m) => m.name === "planning_template")!,
          ),
      );
      expect(shown, locale).toBeDefined();
      expect(shown!.text, locale).toBe(card.text);
      // An English card on a translated page is marked as English.
      expect(shown!.lang, locale).toBe(locale === "en" ? undefined : "en");
      // The product's own translation of the template ("copied into shifts") is on no card and not in the copy.
      const md = appMirror(planning, locale, messages[locale], "", apps);
      expect(md, locale).toContain(card.text);
      if (locale !== "en") {
        const label = loadLabels(locale, "planning").models["planning_template"]?.description;
        expect(label, `${locale}: the label file has a description to keep away`).toBeTruthy();
        expect(
          v.models.map((c) => c.text),
          locale,
        ).not.toContain(label);
        expect(md, locale).not.toContain(label!);
      }
    }
  });

  test("limitsOf also takes the limit of a model card, headed by the card's title or the model's name", () => {
    // One model card of data/apps.ts carries a limit today (Fleet's driver, below); this synthetic app also
    // tries the order after the app's own limit and a titled model card, which no real app has.
    const model = crm.models[0]!;
    const app = made({
      limits: ["A limit of the whole app."],
      cards: {
        [model.name]: { text: "A plain sentence on the card.", limit: "A limit of the model." },
      },
    });
    expect(limitsOf(app).map((l) => [l.head, l.text])).toEqual([
      [undefined, "A limit of the whole app."],
      [modelLabel(app, model), "A limit of the model."],
    ]);
    const titled = made({
      cards: {
        [model.name]: {
          title: "Lead card",
          text: "A plain sentence on the card.",
          limit: "A limit of the model.",
        },
      },
    });
    expect(limitsOf(titled)).toEqual([{ head: "Lead card", text: "A limit of the model." }]);
    expect(appMirror(titled, "en", messages.en, "", apps)).toContain(
      "- **Lead card**: A limit of the model.",
    );
  });

  test("the head of a limit is the heading of its card, and a limit of the app has none", () => {
    const eco = apps.find((a) => a.id === "ecommerce")!;
    expect(limitsOf(eco).map((l) => l.head)).toEqual(["Order: refund", "Order: refund delivered"]);
    // One list item per line, so the two refunds are two items and not one.
    const md = appMirror(eco, "en", en, "", apps);
    const lines = md.slice(cut(md)).split("\n");
    expect(lines.filter((l) => l.startsWith("- **Order: refund"))).toHaveLength(2);
    const planning = apps.find((a) => a.id === "planning")!;
    expect(limitsOf(planning)).toEqual([{ text: planning.limits[0]! }]);
    // A model card's limit is headed by the card's title, the model's name without the app prefix.
    const fleet = apps.find((a) => a.id === "fleet")!;
    expect(limitsOf(fleet)).toEqual([
      { head: "Vehicle driver", text: fleet.cards["fleet_vehicle_driver"]!.limit! },
    ]);
    for (const locale of ["en", "de"] as const) {
      const m = messages[locale];
      for (const app of apps.filter((a) => limitsOf(a).some((l) => l.head))) {
        const v = appView(app, m, apps, locale);
        const md = appMirror(app, locale, m, "", apps);
        // The same words as the card's heading (a translated page keeps the card's capital; the
        // limit is English prose and lowers it).
        const heads = [...v.actions, ...v.models].map((c) =>
          (c.kicker ? `${c.kicker}: ${c.title}` : c.title).toLowerCase(),
        );
        for (const l of limitsOf(app).filter((x) => x.head)) {
          expect(heads, `${locale} ${app.id} ${l.head}`).toContain(l.head!.toLowerCase());
          expect(md, `${locale} ${app.id}`).toContain(`- **${l.head}**: ${l.text}`);
        }
      }
    }
  });

  test("no heading is added and no id changes: the headings are those of an app without limits", () => {
    const headings = (md: string) => md.split("\n").filter((l) => /^#{1,6} /.test(l));
    for (const app of apps.filter((a) => limitsOf(a).length > 0))
      for (const locale of LOCALES) {
        const m = messages[locale];
        const plain: App = {
          ...app,
          limits: [],
          cards: Object.fromEntries(
            Object.entries(app.cards).map(([k, c]) => [k, { ...c, limit: undefined }]),
          ) as App["cards"],
        };
        expect(limitsOf(plain), `${locale} ${app.id}`).toEqual([]);
        for (const mirror of [true, false])
          expect(
            headings(developerBody(app, locale, m, mirror, "", apps)),
            `${locale} ${app.id}`,
          ).toEqual(headings(developerBody(plain, locale, m, mirror, "", apps)));
      }
  });

  test("on a translated page the limits are English and marked so; on an English page and in the copy there is no mark", () => {
    const eco = apps.find((a) => a.id === "ecommerce")!;
    const limit = eco.cards["ecommerce_order.refund"]!.limit!;
    for (const locale of LOCALES) {
      const m = messages[locale];
      const page = developerBody(eco, locale, m, false, "", apps);
      const copy = developerBody(eco, locale, m, true, "", apps);
      const at = page.indexOf(limit);
      expect(at, locale).toBeGreaterThan(0);
      expect(copy, locale).toContain(limit);
      expect(copy, `${locale}: no HTML in the copy`).not.toContain("en-note");
      const before = page.slice(0, at);
      if (locale === "en") expect(page, locale).not.toContain("en-note");
      else {
        // The nearest English note before the list is still open where the list starts.
        const open = before.lastIndexOf('<div lang="en" class="en-note"');
        expect(open, locale).toBeGreaterThan(-1);
        expect(before.indexOf("</div>", open), `${locale}: the list is inside the note`).toBe(-1);
      }
    }
  });

  // The ratchet of false sentences reads what a visitor reads. The Markdown copy has the limits
  // (above), and so does the page, which is the same list inside the English note.
  test("the wordings on the list of false sentences are not in the folded part of any page either, in any language", () => {
    for (const app of apps)
      for (const locale of LOCALES) {
        const folded = strip(developerBody(app, locale, messages[locale], false, "", apps));
        for (const { re, why } of CLAIMS)
          expect(folded, `${locale} ${app.id} ${re}: ${why}`).not.toMatch(re);
      }
  });

  test("the ratchet reaches a limit: a false sentence put in one is found on the page and in the copy", () => {
    const invoicing = apps.find((a) => a.id === "invoicing")!;
    const bad: App = {
      ...invoicing,
      limits: [...invoicing.limits, "Then finalise each one to give it its own number."],
    };
    const claim = CLAIMS.find((c) => c.re.test("finalise each one"))!;
    expect(claim).toBeDefined();
    for (const locale of LOCALES) {
      const page = strip(developerBody(bad, locale, messages[locale], false, "", apps));
      expect(page, locale).toMatch(claim.re);
      expect(appMirror(bad, locale, messages[locale], "", apps), locale).toMatch(claim.re);
    }
    expect(strip(developerBody(invoicing, "en", en, false, "", apps))).not.toMatch(claim.re);
  });
});
