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
]);

function sources(dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) sources(p, out);
    else if (/\.tsx?$/.test(e.name) && !/\.(test|d)\.tsx?$/.test(e.name)) out.push(p);
  }
  return out;
}

const WORDED_INTO_STATE = /\bset[A-Z]\w*\(\s*(?:t|tx|serverMessage)\(/g;

/** The setter calls of a text that hand it a finished sentence, as `line: code`. */
function sentencesKept(text: string): string[] {
  const hits: string[] = [];
  for (const m of text.matchAll(WORDED_INTO_STATE)) {
    const line = text.slice(0, m.index).split("\n").length;
    hits.push(`${line}: ${m[0].replace(/\s+/g, " ")}`);
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
