import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vite-plus/test";

// A sentence worded once and kept in state keeps the language it was made in: after a switch the
// screen shows it in the old one (docs/ISSUES.md D9). A screen keeps the words instead
// (`useWorded` with `() => t(...)`, or the raw message worded where it is drawn). This scan finds
// a state setter handed a finished sentence: `setError(t(...))`, `setProblem(serverMessage(...))`.
const SRC = fileURLToPath(new URL(".", import.meta.url));

/** Files that still keep sentences, each waiting for its own change (D9); none may be added. */
const OPEN = new Set([
  // Screens other open pull requests may change too; converted after those are merged.
  "AgentTokens.tsx",
  "Identity.tsx",
  "Security.tsx",
]);

function sources(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) sources(p, out);
    else if (/\.tsx?$/.test(e.name) && !/\.(test|d)\.tsx?$/.test(e.name)) out.push(p);
  }
  return out;
}

const SETTER = /\bset[A-Z]\w*\(/g;
const WORDING = /^(?:t|tx|serverMessage)\(/;
const QUOTES = new Set(['"', "'", "`"]);

/** The index just past a string literal that opens at `i`. */
function pastString(s: string, i: number): number {
  const q = s[i];
  for (i++; i < s.length && s[i] !== q; i++) if (s[i] === "\\") i++;
  return i;
}

/** The text of a call's arguments, from `open` (just after its parenthesis) to its close. */
function argumentsOf(text: string, open: number): string {
  let depth = 1;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (QUOTES.has(c)) i = pastString(text, i);
    else if ("([{".includes(c)) depth++;
    else if (")]}".includes(c) && --depth === 0) return text.slice(open, i);
  }
  return text.slice(open);
}

/**
 * Whether the arguments of a setter word a sentence now: a `t(`, `tx(` or `serverMessage(` that
 * no arrow function defers, found anywhere in them, also as `{ text: t(...) }` or
 * `a ? serverMessage(m) : ""`. An arrow defers its body: from `=>` to the next comma at its own
 * depth or the end of the bracket it is in.
 */
function wordsNow(args: string): boolean {
  let depth = 0;
  const arrows: number[] = []; // the depths at which an arrow's body is open
  for (let i = 0; i < args.length; i++) {
    const c = args[i];
    if (QUOTES.has(c)) i = pastString(args, i);
    else if ("([{".includes(c)) depth++;
    else if (")]}".includes(c)) {
      depth--;
      while (arrows.length && arrows[arrows.length - 1] > depth) arrows.pop();
    } else if (c === ",") {
      while (arrows.length && arrows[arrows.length - 1] >= depth) arrows.pop();
    } else if (c === "=" && args[i + 1] === ">") {
      arrows.push(depth);
      i++;
    } else if (!arrows.length && !/[\w$.]/.test(args[i - 1] ?? "")) {
      if (WORDING.test(args.slice(i, i + 16))) return true;
    }
  }
  return false;
}

/** The setter calls of a text that hand it a finished sentence, as `line: code`. */
function sentencesKept(text: string): string[] {
  const hits: string[] = [];
  for (const m of text.matchAll(SETTER)) {
    const args = argumentsOf(text, m.index + m[0].length);
    if (!wordsNow(args)) continue;
    const line = text.slice(0, m.index).split("\n").length;
    hits.push(`${line}: ${(m[0] + args).replace(/\s+/g, " ").slice(0, 90)}`);
  }
  return hits;
}

describe("sentences kept in state (D9)", () => {
  it("the scan finds a sentence handed to a setter, and not the words of one", () => {
    expect(sentencesKept('setError(t("Choose a file"));')).toHaveLength(1);
    expect(sentencesKept("setProblem(\n  serverMessage(r.error));")).toHaveLength(1);
    expect(sentencesKept('setError(() => t("Choose a file"));')).toEqual([]);
    expect(sentencesKept("setError(r.message);")).toEqual([]);
    expect(sentencesKept('d.notify(t("Saved"));')).toEqual([]);
    // The other forms of the D9 row: a sentence inside the object or the condition handed over.
    expect(sentencesKept('setMessage({ tone: "success", text: t("Saved.") });')).toHaveLength(1);
    expect(sentencesKept('setState({ kind: "error", message: serverMessage(e) });')).toHaveLength(
      1,
    );
    expect(sentencesKept('setError(s.kind === "e" ? serverMessage(s.message) : "");')).toHaveLength(
      1,
    );
    expect(
      sentencesKept('setMessage({\n  tone: "info",\n  text: t("Wait", { a }),\n});'),
    ).toHaveLength(1);
    expect(sentencesKept('setMessage({ a: () => 1, text: t("Saved.") });')).toHaveLength(1);
    expect(sentencesKept('setMessage({ tone: "success", text: () => t("Done.") });')).toEqual([]);
    expect(sentencesKept('setError(() => (a ? serverMessage(m) : t("Needs approval")));')).toEqual(
      [],
    );
    expect(sentencesKept('setNote("a ) t(b");')).toEqual([]);
    expect(sentencesKept('setLabel(item.t("x"));')).toEqual([]);
  });

  it("no screen hands a setter a finished sentence", () => {
    const found: string[] = [];
    for (const file of sources(SRC)) {
      const rel = relative(SRC, file).replace(/\\/g, "/");
      if (OPEN.has(rel)) continue;
      for (const hit of sentencesKept(readFileSync(file, "utf8"))) found.push(`${rel}:${hit}`);
    }
    expect(found).toEqual([]);
  });
});
