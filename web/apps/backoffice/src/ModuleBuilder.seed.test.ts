// D10: the module builder's example is worded in the language shown, also after a change of
// language; what the person typed stays as typed.
import { afterEach, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage, t } from "@obop/ui";
import { SEED, shownField, type DField } from "./ModuleBuilder.tsx";

afterEach(async () => {
  await setLanguage("en", false);
});

it("words an untouched example field in the language of the moment (D10)", async () => {
  provideCatalogue("en", {});
  provideCatalogue("de", { [SEED.title]: "Was geprüft wurde", [SEED.name]: "Fuhrparkprüfung" });
  const seeded = { name: "title", description: SEED.title, seed: true } as DField;
  const typed = { name: "note", description: "My own words" } as DField;
  await setLanguage("en", false);
  expect(shownField(seeded).description).toBe("What was inspected");
  await setLanguage("de", false);
  expect(shownField(seeded).description).toBe("Was geprüft wurde");
  expect(t(SEED.name)).toBe("Fuhrparkprüfung");
  expect(shownField(typed).description).toBe("My own words");
});
