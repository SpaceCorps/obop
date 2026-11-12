import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vite-plus/test";
import { LOCALES, messages } from "../../i18n/index.ts";
import { REPO_ROOT } from "./paths.ts";

// The Get box of a marketplace app sends the reader to Customize, then Marketplace. A reader who
// follows it must find those words in the back office, in their own language, so the box uses the
// back office's own strings (the "app" context of Customize is the name of the app in the launcher).
describe("the Get box names Customize and Marketplace as the back office calls them", () => {
  for (const locale of LOCALES) {
    test(locale, () => {
      const file = join(REPO_ROOT, "web/apps/backoffice/locales", locale, "messages.json");
      const catalogue = JSON.parse(readFileSync(file, "utf8")) as Record<string, string>;
      const customize = catalogue["app\u0004Customize"] ?? catalogue["Customize"];
      const marketplace = catalogue["Marketplace"];
      expect(customize, `${locale} Customize in the back office`).toBeTruthy();
      expect(marketplace, `${locale} Marketplace in the back office`).toBeTruthy();
      const lead = messages[locale].apps.store.getLead;
      expect(lead, `${locale} getLead`).toContain(customize);
      expect(lead, `${locale} getLead`).toContain(marketplace);
    });
  }
});

describe("the strings of the apps pages", () => {
  test("the short description does not promise a click count", () => {
    // Installing needs an administrator and a plan the administrator confirms once.
    for (const locale of LOCALES)
      expect(messages[locale].apps.description, locale).not.toMatch(
        /click|klick|clic|klik|napsau|kliknię|a un clic|ett klick/i,
      );
  });
});

// The owner ruled on 2026-11-11 that the site says obop is below version 1.0 and not "beta": in the
// footer of every page, on the pill of the home page and in the home page's answer to whether obop is
// ready for production. This is true of all seven languages, not only English. (The word is still right
// for an app that the manifest calls beta, so the FAQ answer may name the maturity label of the language
// once, as the label and nothing else; llms.txt and the built pages are read by the browser tests.)
describe("obop itself is below version 1.0 and not beta, in every language", () => {
  const OLD = /beta|bêta/i;
  for (const locale of LOCALES) {
    test(locale, () => {
      const m = messages[locale];
      for (const [where, text] of [
        ["the footer note", m.ui.betaNote],
        ["the pill", m.home.badge],
      ] as const) {
        expect(text, `${locale}: ${where}`).not.toMatch(OLD);
        expect(text, `${locale}: ${where} names the version`).toMatch(/1\.0/);
      }
      const answers = m.home.faq.filter((f) => f.a.includes("1.0"));
      expect(answers, `${locale}: the answer about production names version 1.0`).toHaveLength(1);
      // The answer may name the maturity label "Beta" of the other apps once (exactly as the language
      // writes it, capital letter included); any further "beta" would be about obop itself.
      expect(
        answers[0]!.a.match(/beta|bêta/gi) ?? [],
        `${locale}: the answer about production names beta only as the label of the other apps`,
      ).toEqual([m.apps.meta.maturities.beta]);
    });
  }
});

// SpaceCorps hosts no obop for other people today (OR-11); a sandbox and paid hosting are planned, not
// offered (OR-20, OR-22, D83); the owner confirmed in chat on 2026-11-11 that the hosted beta is not
// live. So nothing may say that something is free during the beta, and no call to action may
// offer a free start or an instance of one's own. The words are each language's own; the old strings
// of every language are the examples the rules must catch.
const FREE: Record<(typeof LOCALES)[number], RegExp> = {
  en: /(?<!\p{L})free(?!\p{L})/iu,
  de: /kostenlos|gratis/i,
  es: /gratis|gratuit/i,
  fi: /ilmai|maksuton|maksutta/i,
  fr: /gratuit/i,
  pl: /bezpłatn|darm/i,
  sv: /gratis|kostnadsfri|avgiftsfri/i,
};
// "Beta" in each language, as a word: "Betalning" (Swedish for payment) is not it.
const BETA = /(?<!\p{L})(?:b[eê]ta(?!l[andt])\p{L}*|bet[yęą]|becie)(?!\p{L})/iu;
const INSTANCE = /instan(?:ce|z|cia|ssi|cj|s)/i;
const OLD: Record<(typeof LOCALES)[number], { beta: string[]; cta: string[] }> = {
  en: {
    beta: ["Everything is free during the beta.", "Free while in beta."],
    cta: ["Start free", "Get your own instance"],
  },
  de: {
    beta: [
      "Während der Beta ist alles kostenlos.",
      "Kostenlos während der Beta.",
      "Kostenlos in der Betaphase.",
    ],
    cta: ["Kostenlos starten", "Eigene Instanz erhalten"],
  },
  es: {
    beta: ["Todo es gratis durante la beta.", "Gratis durante la beta."],
    cta: ["Empezar gratis", "Obtén tu propia instancia"],
  },
  fi: {
    beta: [
      "Kaikki on ilmaista beta-vaiheen ajan.",
      "Maksuton betavaiheen ajan.",
      "Ilmainen betassa.",
    ],
    cta: ["Aloita ilmaiseksi", "Hanki oma instanssi"],
  },
  fr: {
    beta: ["Tout est gratuit pendant la bêta.", "Gratuit pendant la bêta."],
    cta: ["Commencer gratuitement", "Obtenir votre instance"],
  },
  pl: {
    beta: [
      "W czasie bety wszystko jest bezpłatne.",
      "Bezpłatnie w okresie beta.",
      "Za darmo w becie.",
      "Za darmo z betą.",
    ],
    cta: ["Zacznij za darmo", "Załóż własną instancję"],
  },
  sv: {
    beta: ["Allt är gratis under betan.", "Gratis under betan.", "Gratis under betaperioden."],
    cta: ["Kom igång gratis", "Skaffa din egen instans"],
  },
};

const sentences = (text: string) => text.split(/(?<=[.!?])\s+/);
const freeDuringBeta = (locale: (typeof LOCALES)[number], text: string) =>
  sentences(text).filter((s) => FREE[locale].test(s) && BETA.test(s));
/** Every string of a message tree, with its path. */
function strings(value: unknown, path = ""): [string, string][] {
  if (typeof value === "string") return [[path, value]];
  if (Array.isArray(value)) return value.flatMap((v, i) => strings(v, `${path}[${i}]`));
  if (value && typeof value === "object")
    return Object.entries(value).flatMap(([k, v]) => strings(v, path ? `${path}.${k}` : k));
  return [];
}

describe("there is no hosted beta: nothing is free during the beta, and no call to action offers an instance", () => {
  for (const locale of LOCALES) {
    test(locale, () => {
      // The rules can fail: each catches the old strings of its language, and not today's maturity label.
      for (const old of OLD[locale].beta)
        expect(freeDuringBeta(locale, old), `${locale}: ${old}`).toHaveLength(1);
      expect(BETA.test("Betalning")).toBe(false);
      const m = messages[locale];
      const cta = {
        "ui.startFree": m.ui.startFree,
        "home.ctaPrimary": m.home.ctaPrimary,
        "home.stepsCta": m.home.stepsCta,
        "home.ctaButton": m.home.ctaButton,
        "pricing.cta": m.pricing.cta,
        "apps.store.open": m.apps.store.open,
        "apps.store.openObop": m.apps.store.openObop,
      };
      const offers = (text: string) => FREE[locale].test(text) || INSTANCE.test(text);
      for (const old of OLD[locale].cta) expect(offers(old), `${locale}: ${old}`).toBe(true);

      // The website's strings.
      for (const [path, text] of strings(m))
        expect(freeDuringBeta(locale, text), `${locale}: ${path}`).toEqual([]);
      for (const [key, text] of Object.entries(cta))
        expect(offers(text), `${locale}: ${key} "${text}"`).toBe(false);

      // The sign-up app's catalogue (its English ids are its English text).
      const file = join(REPO_ROOT, "web/apps/platform/locales", locale, "messages.json");
      const catalogue = JSON.parse(readFileSync(file, "utf8")) as Record<string, string>;
      for (const [id, text] of Object.entries(catalogue)) {
        expect(freeDuringBeta("en", id), `the sign-up app: ${id}`).toEqual([]);
        expect(freeDuringBeta(locale, text), `the sign-up app, ${locale}: ${id}`).toEqual([]);
      }
    });
  }
});
