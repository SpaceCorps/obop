// @vitest-environment jsdom
// The count sentences and the bulk-create sentence have real plural forms in every language, so a
// screen reader's name and the menu text agree with their number (D97, review of the page).
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, t } from "@obop/ui";
import pl from "../../locales/pl/messages.json";
import de from "../../locales/de/messages.json";

const COUNT = "{n, plural, one {# waiting for you} other {# waiting for you}}";
const BULK =
  "{count, plural, one {# record will be created. The first one:} other {# records will be created. The first ones:}}";

afterEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
});

describe("plural forms of the approvals sentences", () => {
  it("Polish says czeka for 1 and 5, czekają for 2 to 4", async () => {
    provideCatalogue("pl", pl as Record<string, string>);
    await setLanguage("pl", false);
    expect(t(COUNT, { n: 1 })).toMatch(/1 czeka na Ciebie$/);
    expect(t(COUNT, { n: 2 })).toMatch(/2 czekają na Ciebie$/);
    expect(t(COUNT, { n: 5 })).toMatch(/5 czeka na Ciebie$/);
    expect(t(COUNT, { n: 22 })).toMatch(/22 czekają na Ciebie$/);
    expect(t(BULK, { count: 3 })).toContain("Zostaną utworzone 3 rekordy");
  });

  it("German says wartet for one and warten for more; the bulk sentence has a singular", async () => {
    provideCatalogue("de", de as Record<string, string>);
    await setLanguage("de", false);
    expect(t(COUNT, { n: 1 })).toBe("1 wartet auf Sie");
    expect(t(COUNT, { n: 3 })).toBe("3 warten auf Sie");
    expect(t(BULK, { count: 1 })).toBe("Es wird 1 Datensatz angelegt. Der erste:");
    expect(t(BULK, { count: 12 })).toBe("Es werden 12 Datensätze angelegt. Die ersten:");
  });

  it("English reads the same for every number", () => {
    expect(t(COUNT, { n: 1 })).toBe("1 waiting for you");
    expect(t(BULK, { count: 1 })).toBe("1 record will be created. The first one:");
  });
});
