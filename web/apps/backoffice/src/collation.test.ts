import { afterEach, describe, expect, it } from "vite-plus/test";
import { compareText, setLanguage, type Lang } from "@obop/ui";
import fixtures from "./collation.fixtures.json";

// The server sorts text with the CLDR collation of `icu_collator`; the browser sorts with the
// engine's Intl.Collator (numeric, base sensitivity, then the raw text). Both read these fixtures:
// crates/kernel/obop-commands/src/tests/polish.rs checks the server's order, this file the
// browser's, so the same page sorts the same way on both sides (D3).
const LANGS: Record<string, Lang> = {
  "": "en",
  en: "en",
  de: "de",
  sv: "sv",
  fi: "fi",
  pl: "pl",
};

afterEach(() => setLanguage("en", false));

describe("text sort order", () => {
  for (const f of fixtures) {
    it(f.name, async () => {
      await setLanguage(LANGS[f.lang]!, false);
      const sorted = [...f.input].sort(compareText);
      expect(sorted).toEqual(f.expected);
    });
  }
});
