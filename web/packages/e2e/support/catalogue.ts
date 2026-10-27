// What the catalogues say for an English message: tests follow the wording of the translation, not a
// copy of it, so a reworded translation does not break them.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { repoRoot } from "./env.ts";

/** The catalogue of a language: the back office's own (`backoffice`), the shared one (`ui`) or another app's. */
export function catalogue(lang: string, bundle = "backoffice"): Record<string, string> {
  const dir = bundle === "ui" ? "packages/ui" : `apps/${bundle}`;
  return JSON.parse(
    readFileSync(resolve(repoRoot, `web/${dir}/locales/${lang}/messages.json`), "utf8"),
  );
}

/** The translation of an English message; throws when there is none, so a typo cannot pass as English. */
export const tr = (lang: string, id: string, bundle = "backoffice") => {
  const text = catalogue(lang, bundle)[id];
  if (!text) throw new Error(`no ${lang} translation of ${JSON.stringify(id)}`);
  return text;
};
