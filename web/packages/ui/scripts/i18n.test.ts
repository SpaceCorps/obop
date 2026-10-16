import { describe, expect, it } from "vite-plus/test";
import { checkAll, checkLocale, extractFile, requiredPlural, root } from "./i18n.ts";

describe("the catalogues", () => {
  it("are complete and consistent in every language (the same check as `vp run i18n:check`)", () => {
    expect(checkAll()).toEqual([]);
  });
});

describe("the extractor", () => {
  const found = (src: string) => extractFile(`${root}/web/x.tsx`, src).found.map((f) => f.key);

  it("finds t, tx, rich and msg calls with any quotes and line breaks", () => {
    const src = `
      t("Save"); t('Cancel'); t(\`Close\`);
      tx("field", "Stage");
      rich(
        "Press <kbd>Esc</kbd>",
        {},
      );
      const x = msg("List"); msgx("option", "Draft");
      other.t("Not mine"); notes("Not mine either");
      t(dynamicId);
    `;
    expect(found(src)).toEqual([
      "Save",
      "Cancel",
      "Close",
      "field\u0004Stage",
      "Press <kbd>Esc</kbd>",
      "List",
      "option\u0004Draft",
    ]);
  });

  it("unescapes the literal and refuses a message built with +", () => {
    expect(found(`t("Say \\"hi\\" \\u2026")`)).toEqual(['Say "hi" …']);
    expect(extractFile(`${root}/web/x.tsx`, `t("a " + b)`).errors.length).toBe(1);
  });
});

describe("the catalogue check", () => {
  const en = {
    "{count, plural, one {# record} other {# records}}":
      "{count, plural, one {# record} other {# records}}",
    "Hello {name}": "Hello {name}",
    Save: "Save",
    Kanban: "Kanban",
  };
  const ok = {
    "{count, plural, one {# record} other {# records}}":
      "{count, plural, one {# rekord} few {# rekordy} many {# rekordów} other {# rekordu}}",
    "Hello {name}": "Cześć {name}",
    Save: "Zapisz",
    Kanban: "Kanban",
  };
  const problems = (tr: Record<string, string>, lang = "pl") =>
    checkLocale(lang, en, tr, { "*": ["Kanban"] }, "t");

  it("accepts a correct translation", () => {
    expect(problems(ok)).toEqual([]);
  });

  it("reports a missing key, an extra key, a wrong placeholder and an untranslated string", () => {
    const text = problems({
      ...ok,
      "Hello {name}": "Cześć {nazwa}",
      Save: "Save",
      Extra: "x",
    }).join("\n");
    expect(text).toContain("placeholders {nazwa} but English has {name}");
    expect(text).toContain("identical to English");
    expect(text).toContain('not in English "Extra"');
    expect(problems({ Save: "Zapisz" }).join("\n")).toContain("missing");
  });

  it("requires every plural category of the language", () => {
    const text = problems({
      ...ok,
      "{count, plural, one {# record} other {# records}}":
        "{count, plural, one {# rekord} other {# rekordów}}",
    }).join("\n");
    expect(text).toContain("plural lacks few, many");
    expect(requiredPlural("pl")).toEqual(["few", "many", "one", "other"]);
    expect(requiredPlural("fi")).toEqual(["one", "other"]);
    expect(requiredPlural("fr")).toEqual(["one", "other"]);
    expect(requiredPlural("sv")).toEqual(["one", "other"]);
  });
});
