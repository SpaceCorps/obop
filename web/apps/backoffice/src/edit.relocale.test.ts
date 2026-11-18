// D11: a decimal typed before a change of language is written again with the new separators.
import { expect, it } from "vite-plus/test";
import { relocaleNumber } from "./edit.ts";

const en = { decimal: ".", group: "," };
const de = { decimal: ",", group: "." };
const sv = { decimal: ",", group: "\u00a0" };

it("keeps the number that was typed when the separators change (D11)", () => {
  expect(relocaleNumber("1,234", en, de)).toBe("1.234");
  expect(relocaleNumber("1.234", en, de)).toBe("1,234");
  expect(relocaleNumber("1,234.5", en, de)).toBe("1.234,5");
  expect(relocaleNumber("-12.50", en, sv)).toBe("-12,50");
  expect(relocaleNumber("1.234,5", de, en)).toBe("1,234.5");
  expect(relocaleNumber("abc", en, de)).toBe("abc");
  expect(relocaleNumber("1.5", en, en)).toBe("1.5");
});

it("Save reads the rewritten text as the number typed (D11)", async () => {
  const { fmt, provideCatalogue, setLanguage } = await import("@obop/ui");
  const { fromEdit } = await import("./edit.ts");
  provideCatalogue("en", {});
  provideCatalogue("de", {});
  const f = {
    name: "qty",
    kind: "decimal" as const,
    scale: 3,
    description: "",
    class: "none",
    personal_data: false,
    required: false,
  };
  await setLanguage("en", false);
  const was = fmt.separators();
  const typedEn = fromEdit(f, "1,234");
  await setLanguage("de", false);
  const now = fmt.separators();
  expect(fromEdit(f, relocaleNumber("1,234", was, now))).toEqual(typedEn);
  expect(fromEdit(f, relocaleNumber("1.234", was, now))).toEqual({ value: "1.234" });
  await setLanguage("en", false);
});
