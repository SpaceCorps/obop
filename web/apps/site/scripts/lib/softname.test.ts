import { describe, expect, test } from "vite-plus/test";
import { SOFT_NAMES, softName } from "../../.vitepress/theme/softname.ts";
import { LOCALES } from "../../i18n/index.ts";
import { appName, loadApps } from "./apps.ts";

const SHY = "­";
const names = [
  ...new Set(
    loadApps().flatMap((app) => [app.name, ...LOCALES.map((locale) => appName(app, locale))]),
  ),
];

describe("a long app name may break at the seams of its compound, and nowhere else is changed", () => {
  test("every entry is a real name, and adds soft hyphens and nothing else", () => {
    for (const name of SOFT_NAMES) {
      expect(names, name).toContain(name);
      expect(softName(name), name).toContain(SHY);
      expect(softName(name).replaceAll(SHY, ""), name).toBe(name);
    }
    expect(softName("CRM")).toBe("CRM");
    expect(softName("Fakturering")).toBe("Fakturering");
  });

  test("every name with a word of 16 letters or more has its seams (the page marks such a name 'long')", () => {
    const long = names.filter((n) => n.split(/[\s-]+/).some((w) => w.length >= 16));
    expect(long.length).toBeGreaterThan(3);
    for (const name of long)
      expect(SOFT_NAMES, `${name} has no seams in softname.ts`).toContain(name);
  });
});
