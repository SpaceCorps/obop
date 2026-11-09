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
