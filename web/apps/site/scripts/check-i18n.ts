// Structural check of the translations, read-only: `node scripts/check-i18n.ts [locale ...]`.
// Messages must have the same keys, array lengths and link targets as English; translated Markdown
// must keep the same code blocks, link targets and front matter keys as its English source.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { LOCALES, messages } from "../i18n/index.ts";
import { SITE_ROOT } from "./lib/paths.ts";

const problems: string[] = [];
const only = process.argv.slice(2);

function shape(a: unknown, b: unknown, path: string, where: string) {
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) {
      problems.push(`${where}: ${path} should be an array of ${a.length}`);
      return;
    }
    a.forEach((x, i) => shape(x, b[i], `${path}[${i}]`, where));
  } else if (a && typeof a === "object") {
    if (!b || typeof b !== "object") {
      problems.push(`${where}: ${path} should be an object`);
      return;
    }
    for (const k of Object.keys(a))
      shape(
        (a as Record<string, unknown>)[k],
        (b as Record<string, unknown>)[k],
        `${path}.${k}`,
        where,
      );
    for (const k of Object.keys(b))
      if (!(k in a)) problems.push(`${where}: unexpected key ${path}.${k}`);
  } else if (typeof a !== "string" || a === "") {
    if (a !== b) problems.push(`${where}: ${path} must stay ${JSON.stringify(a)}`);
  } else if (typeof b !== "string" || b.trim() === "") {
    problems.push(`${where}: ${path} is empty or not a string`);
  } else if (path.endsWith(".href") && a !== b) {
    problems.push(`${where}: ${path} must stay ${String(a)}`);
  } else if (path.endsWith(".icon") && a !== b) {
    problems.push(`${where}: ${path} must stay ${String(a)}`);
  }
}

function walk(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : p.endsWith(".md") ? [p] : [];
  });
}
const links = (s: string) =>
  [...s.matchAll(/\]\((\/[^)\s]*)\)/g)]
    .map((m) => m[1])
    .sort()
    .join(" ");
const fences = (s: string) => [...s.matchAll(/```[\s\S]*?```/g)].map((m) => m[0]);
const fmKeys = (s: string) =>
  [...(/^---\n([\s\S]*?)\n---/.exec(s)?.[1] ?? "").matchAll(/^([\w-]+):/gm)]
    .map((m) => m[1])
    .sort()
    .join(",");

for (const l of LOCALES) {
  if (l === "en" || (only.length && !only.includes(l))) continue;
  const m = messages[l];
  shape(messages.en, m, "m", l);
  if (m.htmlLang !== l) problems.push(`${l}: htmlLang should be ${l}`);
  const dir = join(SITE_ROOT, "content", l);
  for (const f of walk(dir)) {
    const rel = relative(dir, f);
    const en = join(SITE_ROOT, "content/en", rel);
    if (!existsSync(en)) {
      problems.push(`${l}/${rel}: no English source`);
      continue;
    }
    const a = readFileSync(en, "utf8");
    const b = readFileSync(f, "utf8");
    if (fmKeys(a) !== fmKeys(b))
      problems.push(`${l}/${rel}: front matter keys differ (${fmKeys(a)} vs ${fmKeys(b)})`);
    if (links(a) !== links(b)) problems.push(`${l}/${rel}: internal link targets differ`);
    const fa = fences(a);
    const fb = fences(b);
    if (fa.length !== fb.length)
      problems.push(`${l}/${rel}: ${fa.length} code blocks expected, found ${fb.length}`);
    else
      fa.forEach((x, i) => {
        // Comments inside code may be translated; the code lines may not change.
        const strip = (s: string) =>
          s
            .split("\n")
            .filter((ln) => !/^\s*(#|\/\/)/.test(ln))
            .join("\n");
        if (strip(x) !== strip(fb[i]!))
          problems.push(`${l}/${rel}: code block ${i + 1} was changed`);
      });
    if (/\n#\s/.test(b.replace(/```[\s\S]*?```/g, "")))
      problems.push(`${l}/${rel}: do not add an H1`);
  }
}
if (problems.length) {
  console.log(problems.join("\n"));
  process.exit(1);
}
console.log("translations: structure ok");
