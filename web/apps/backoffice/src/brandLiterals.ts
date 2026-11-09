// The reader behind `brandLiterals.test.ts`. It is test support, not part of the app: nothing imports
// it but that test, so the build leaves it out.
//
// An instance can have a brand of its own (docs/spec/branding.md), and then no screen of the back
// office may say `obop` to its people. The scan cannot know which of the words in the sources a
// person meets, so it makes a new one a deliberate act: every string literal, template text and
// piece of JSX text that holds the word, and every catalogue value that does, has to be a row of
// the table in the test, with the reason it is allowed. It reads sources the way `i18nGuard.ts`
// does (`texts` and `code`), so a comment is not a literal.

import { code, texts } from "./i18nGuard.ts";

export type Hit = {
  /** The path, relative to `web/`. */
  file: string;
  /** 1-based line of the word. */
  line: number;
  /**
   * `string` and `template`: the text of the literal. `code`: the line of code that holds the word
   * by itself, which is JSX text or an identifier. `catalogue`: the English message the value
   * belongs to.
   */
  kind: "string" | "template" | "code" | "catalogue";
  text: string;
};

const WORD = /obop/i;
const lineOf = (src: string, at: number) => src.slice(0, at).split("\n").length;

/** The hits of one `.ts` or `.tsx` file. */
export function sourceHits(file: string, src: string): Hit[] {
  const hits: Hit[] = [];
  for (const t of texts(src)) {
    if (t.kind !== "string" && t.kind !== "template") continue;
    const text = src.slice(t.from, t.to);
    // The module a statement imports or re-exports from is a name, not a text.
    if (
      t.kind === "string" &&
      /(?:\bfrom|\bimport\s*\(?)\s*["']$/.test(src.slice(Math.max(0, t.from - 24), t.from))
    )
      continue;
    if (WORD.test(text)) hits.push({ file, line: lineOf(src, t.from), kind: t.kind, text });
  }
  // What is left of the word in the code once strings, templates, regular expressions and comments
  // are blanked is JSX text (`<p>obop</p>`) or a name. A longer name (`obopRecords`, `OBOP_X`) is
  // not the word on its own and is not a hit.
  const masked = code(src);
  for (const m of masked.matchAll(/(?<![\w$])obop(?![\w$])/gi)) {
    const start = masked.lastIndexOf("\n", m.index) + 1;
    const end = masked.indexOf("\n", m.index);
    hits.push({
      file,
      line: lineOf(src, m.index),
      kind: "code",
      text: src.slice(start, end < 0 ? src.length : end).trim(),
    });
  }
  return hits.sort((a, b) => a.line - b.line);
}

/**
 * The hits of one catalogue: every message whose English id, or whose text in this language, holds
 * the word. The row of the table is the English message, whatever language the hit is in.
 */
export function catalogueHits(file: string, messages: Record<string, string>): Hit[] {
  return Object.entries(messages)
    .filter(([id, text]) => WORD.test(id) || WORD.test(text))
    .map(([id]) => ({ file, line: 0, kind: "catalogue" as const, text: id }));
}

/**
 * The files in `src` that import `Logo` by name. `Logo` is obop's own mark, and the apps draw the
 * instance's logo through `InstanceLogo`, which shows it when there is no brand.
 */
export function logoImports(src: string): boolean {
  const masked = code(src);
  for (const m of masked.matchAll(/import\s+(?:type\s+)?\{([^}]*)\}\s*from\s*["'][^"']*["']/g)) {
    const list = m[1]!.split(",").map((s) =>
      s
        .trim()
        .replace(/^type\s+/, "")
        .split(/\s+as\s+/)[0]!
        .trim(),
    );
    if (list.includes("Logo")) return true;
  }
  return false;
}
