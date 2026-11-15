import { createMarkdownRenderer } from "vitepress";
import { describe, expect, test } from "vite-plus/test";
import { LOCALES, messages } from "../../i18n/index.ts";
import { slugify } from "./slug.ts";

/**
 * `slug.ts` is a hand copy of VitePress's slugify, used so that the layout's "What it is for" heading
 * keeps the id the Markdown heading had. This renders real headings through VitePress's own Markdown
 * pipeline and compares the ids, so a VitePress upgrade that changes slugify fails here and not in a
 * reader's bookmark.
 */
describe("slug.ts gives the ids VitePress gives", () => {
  test("the purpose heading of every language, and a few awkward headings", async () => {
    const md = await createMarkdownRenderer("/", {}, "/");
    const headings = [
      ...LOCALES.map((l) => messages[l].apps.h.purpose),
      "What agents can and cannot do",
      "Wofür es gedacht ist",
      "Co to robi — krótko",
      "Tarkoitus: mihin se on tarkoitettu?",
      "¿Para qué sirve? (en breve)",
      "À quoi ça sert",
      "2 things, “quoted” and ‘single’",
      "Under_score and-dash",
    ];
    for (const h of headings) {
      const id = /<h2 id="([^"]*)"/.exec(md.render(`## ${h}\n`))?.[1];
      expect(slugify(h), h).toBe(id);
    }
  });
});
