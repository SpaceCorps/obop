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
