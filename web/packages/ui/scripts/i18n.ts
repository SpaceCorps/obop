// Catalogue tooling. Run through `vp run i18n:extract|i18n:check|i18n:status` in web/packages/ui.
//
//   extract  scan the sources for t(), tx(), rich(), msg(), msgx() and write locales/en/messages.json
//            of every bundle (the English catalogue is generated, never edited by hand)
//   check    fail when a catalogue lacks a key present in English, has a key not in English, has
//            mismatched placeholders, plural forms or tags, or an untranslated (identical to English)
//            string that is not in locales/identical.json; also fails when locales/en is stale
//   status   count machine-drafted and reviewed strings per locale
//
// Message ids are the English source text; a message with a context has the key `ctx\u0004text`.
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { argNames, parseStrict, pluralCases, tagNames } from "../src/i18n/message.ts";

const here = dirname(fileURLToPath(import.meta.url));
export const root = join(here, "../../../..");
export const CTX = "\u0004";
export const TARGETS = ["de", "fr", "pl", "fi", "sv", "es"] as const;

export type Bundle = { name: string; src: string; out: string };
export const BUNDLES: Bundle[] = [
  { name: "ui", src: "web/packages/ui/src", out: "web/packages/ui/locales" },
  { name: "backoffice", src: "web/apps/backoffice/src", out: "web/apps/backoffice/locales" },
  { name: "platform", src: "web/apps/platform/src", out: "web/apps/platform/locales" },
];

/* ---------------- Extraction ---------------- */

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "node_modules") continue;
      walk(p, out);
      // The runtime in src/i18n documents t() with examples; only its React pickers carry messages.
    } else if (
      /\.(ts|tsx)$/.test(name) &&
      !/\.(test|d)\.tsx?$/.test(name) &&
      (!p.includes("/i18n/") || name === "react.tsx")
    )
      out.push(p);
  }
  return out;
}

function unescapeJs(body: string, quote: string): string {
  return body.replace(
    /\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|x[0-9a-fA-F]{2}|[\s\S])/g,
    (_, e: string) => {
      if (e[0] === "u" && e.length > 1)
        return String.fromCodePoint(parseInt(e.startsWith("u{") ? e.slice(2, -1) : e.slice(1), 16));
      if (e[0] === "x" && e.length > 1) return String.fromCharCode(parseInt(e.slice(1), 16));
      switch (e) {
        case "n":
          return "\n";
        case "t":
          return "\t";
        case "r":
          return "\r";
        case "0":
          return "\0";
        default:
          return e === quote || e === "\\" || e === "`" || e === "'" || e === '"' ? e : e;
      }
    },
  );
}

/** Read a string literal at `i`; returns [value, next index] or null when it is not a plain literal. */
function literal(src: string, i: number): [string, number] | null {
  const q = src[i];
  if (q !== '"' && q !== "'" && q !== "`") return null;
  let j = i + 1;
  let body = "";
  while (j < src.length) {
    const c = src[j]!;
    if (c === "\\") {
      body += c + src[j + 1];
      j += 2;
      continue;
    }
    if (q === "`" && c === "$" && src[j + 1] === "{") return null;
    if (c === q) return [unescapeJs(body, q), j + 1];
    body += c;
    j++;
  }
  return null;
}

const skipWs = (s: string, i: number) => {
  while (i < s.length) {
    if (/\s/.test(s[i]!)) i++;
    else if (s.startsWith("//", i)) i = s.indexOf("\n", i) + 1 || s.length;
    else if (s.startsWith("/*", i)) i = s.indexOf("*/", i) + 2 || s.length;
    else break;
  }
  return i;
};

export type Found = { key: string; file: string; line: number };
export type Extraction = {
  found: Found[];
  dynamic: { file: string; line: number; text: string }[];
  errors: string[];
};

const CALL = /(?<![\w.$])(t|tx|rich|msg|msgx)\(/g;

export function extractFile(file: string, src: string): Extraction {
  const out: Extraction = { found: [], dynamic: [], errors: [] };
  const rel = relative(root, file);
  const lineOf = (idx: number) => src.slice(0, idx).split("\n").length;
  for (const m of src.matchAll(CALL)) {
    const name = m[1]!;
    let i = skipWs(src, m.index! + m[0].length);
    const withCtx = name === "tx" || name === "msgx";
    let ctx = "";
    if (withCtx) {
      const c = literal(src, i);
      if (!c) {
        out.dynamic.push({
          file: rel,
          line: lineOf(m.index!),
          text: src.slice(m.index!, m.index! + 50),
        });
        continue;
      }
      ctx = c[0];
      i = skipWs(src, c[1]);
      if (src[i] !== ",") continue;
      i = skipWs(src, i + 1);
    }
    const id = literal(src, i);
    if (!id) {
      // `t(` followed by an expression: a dynamic call (its ids are marked with msg() elsewhere).
      if (src[i] !== ")")
        out.dynamic.push({
          file: rel,
          line: lineOf(m.index!),
          text: src.slice(m.index!, m.index! + 50),
        });
      continue;
    }
    const after = skipWs(src, id[1]);
    if (src[after] === "+")
      out.errors.push(
        `${rel}:${lineOf(m.index!)} message built with +: write one message with placeholders`,
      );
    out.found.push({
      key: ctx ? `${ctx}${CTX}${id[0]}` : id[0],
      file: rel,
      line: lineOf(m.index!),
    });
  }
  return out;
}

export function extractBundle(b: Bundle): { messages: Record<string, string>; ex: Extraction } {
  const ex: Extraction = { found: [], dynamic: [], errors: [] };
  for (const f of walk(join(root, b.src))) {
    const r = extractFile(f, readFileSync(f, "utf8"));
    ex.found.push(...r.found);
    ex.dynamic.push(...r.dynamic);
    ex.errors.push(...r.errors);
  }
  const keys = [...new Set(ex.found.map((f) => f.key))].sort(
    (a, b) => a.localeCompare(b, "en") || (a < b ? -1 : 1),
  );
  const messages: Record<string, string> = {};
  for (const k of keys) messages[k] = k.includes(CTX) ? k.slice(k.indexOf(CTX) + 1) : k;
  for (const k of keys) {
    try {
      parseStrict(messages[k]!);
    } catch (e) {
      ex.errors.push(`${b.name}: bad message ${JSON.stringify(k)}: ${(e as Error).message}`);
    }
  }
  return { messages, ex };
}

const json = (v: unknown) => JSON.stringify(v, null, 2) + "\n";

function readJson<T>(p: string, fallback: T): T {
  return existsSync(p) ? (JSON.parse(readFileSync(p, "utf8")) as T) : fallback;
}

export function extractAll(write: boolean): string[] {
  const problems: string[] = [];
  for (const b of BUNDLES) {
    const { messages, ex } = extractBundle(b);
    problems.push(...ex.errors);
    const dir = join(root, b.out, "en");
    const file = join(dir, "messages.json");
    const text = json(messages);
    if (write) {
      mkdirSync(dir, { recursive: true });
      writeFileSync(file, text);
      console.log(`${b.name}: ${Object.keys(messages).length} messages -> ${relative(root, file)}`);
    } else if (!existsSync(file) || readFileSync(file, "utf8") !== text) {
      problems.push(`${b.name}: ${relative(root, file)} is stale; run \`vp run i18n:extract\``);
    }
  }
  return problems;
}

/* ---------------- Check ---------------- */

type Allow = Record<string, string[]>;

/** Plural categories a locale must write: those that integers 0..1000 reach, plus `other`. */
export function requiredPlural(lang: string): string[] {
  const pr = new Intl.PluralRules(lang);
  const cats = new Set<string>(["other"]);
  for (let n = 0; n <= 1000; n++) cats.add(pr.select(n));
  return [...cats].sort();
}

const letters = (s: string) =>
  s
    .replace(/\{[^{}]*\}/g, "")
    .replace(/<\/?[a-z0-9]+>/g, "")
    .replace(/\p{P}|\p{N}|\s/gu, "");

export function checkLocale(
  lang: string,
  en: Record<string, string>,
  tr: Record<string, string>,
  allow: Allow,
  label: string,
): string[] {
  const p: string[] = [];
  const missing = Object.keys(en).filter((k) => !(k in tr));
  const extra = Object.keys(tr).filter((k) => !(k in en));
  for (const k of missing) p.push(`${label}/${lang}: missing ${JSON.stringify(k)}`);
  for (const k of extra) p.push(`${label}/${lang}: not in English ${JSON.stringify(k)}`);
  const need = requiredPlural(lang);
  const allowed = new Set([...(allow["*"] ?? []), ...(allow[lang] ?? [])]);
  for (const [k, source] of Object.entries(en)) {
    const v = tr[k];
    if (v === undefined) continue;
    const id = JSON.stringify(k);
    if (typeof v !== "string" || v.trim() === "") {
      p.push(`${label}/${lang}: empty translation of ${id}`);
      continue;
    }
    let ok = true;
    try {
      parseStrict(v);
    } catch (e) {
      ok = false;
      p.push(`${label}/${lang}: ${id}: ${(e as Error).message}`);
    }
    if (!ok) continue;
    const a = argNames(source).join(",");
    const b = argNames(v).join(",");
    if (a !== b) p.push(`${label}/${lang}: ${id}: placeholders {${b}} but English has {${a}}`);
    const ta = tagNames(source).join(",");
    const tb = tagNames(v).join(",");
    if (ta !== tb) p.push(`${label}/${lang}: ${id}: tags <${tb}> but English has <${ta}>`);
    const pa = pluralCases(source);
    const pb = pluralCases(v);
    if (pa.length !== pb.length)
      p.push(`${label}/${lang}: ${id}: ${pb.length} plural(s), English has ${pa.length}`);
    else
      for (const cases of pb) {
        const real = need.filter((c) => !cases.includes(c));
        if (real.length)
          p.push(
            `${label}/${lang}: ${id}: plural lacks ${real.join(", ")} (${lang} needs ${need.join(", ")})`,
          );
      }
    if (source.endsWith("…") !== v.endsWith("…") && !source.endsWith("..."))
      p.push(`${label}/${lang}: ${id}: ellipsis differs from English`);
    if (/^\s|\s$/.test(source) !== /^\s|\s$/.test(v) && source.trim() === source)
      p.push(`${label}/${lang}: ${id}: stray leading or trailing space`);
    if (v === source && letters(source) && !allowed.has(k))
      p.push(
        `${label}/${lang}: ${id}: identical to English (translate it, or list it in locales/identical.json)`,
      );
  }
  return p;
}

export function checkAll(): string[] {
  const problems = extractAll(false);
  const allow = readJson<Allow>(join(root, "web/packages/ui/locales/identical.json"), {});
  for (const b of BUNDLES) {
    const enFile = join(root, b.out, "en/messages.json");
    const en = readJson<Record<string, string>>(enFile, {});
    for (const lang of TARGETS) {
      const dir = join(root, b.out, lang);
      const f = join(dir, "messages.json");
      if (!existsSync(f)) {
        problems.push(`${b.name}/${lang}: ${relative(root, f)} does not exist`);
        continue;
      }
      const tr = readJson<Record<string, string>>(f, {});
      const own = readJson<string[]>(
        join(root, "web/packages/ui/locales", lang, "identical.json"),
        [],
      );
      const allowed: Allow = { "*": allow["*"] ?? [], [lang]: [...(allow[lang] ?? []), ...own] };
      problems.push(...checkLocale(lang, en, tr, allowed, b.name));
      const sf = join(dir, "status.json");
      const st = readJson<Record<string, string>>(sf, {});
      for (const k of Object.keys(tr)) {
        if (!(k in st)) problems.push(`${b.name}/${lang}: status.json lacks ${JSON.stringify(k)}`);
        else if (st[k] !== "machine-drafted" && st[k] !== "reviewed")
          problems.push(
            `${b.name}/${lang}: status of ${JSON.stringify(k)} must be machine-drafted or reviewed`,
          );
      }
      for (const k of Object.keys(st))
        if (!(k in tr))
          problems.push(`${b.name}/${lang}: status.json has stale key ${JSON.stringify(k)}`);
    }
  }
  return problems;
}

/** Make status.json list exactly the keys of messages.json: new keys are machine-drafted. */
export function syncStatus(): number {
  let changed = 0;
  for (const b of BUNDLES) {
    for (const lang of TARGETS) {
      const dir = join(root, b.out, lang);
      const mf = join(dir, "messages.json");
      if (!existsSync(mf)) continue;
      const tr = readJson<Record<string, string>>(mf, {});
      const old = readJson<Record<string, string>>(join(dir, "status.json"), {});
      const next: Record<string, string> = {};
      for (const k of Object.keys(tr))
        next[k] = old[k] === "reviewed" ? "reviewed" : "machine-drafted";
      const text = json(next);
      const sf = join(dir, "status.json");
      if (!existsSync(sf) || readFileSync(sf, "utf8") !== text) {
        writeFileSync(sf, text);
        changed++;
      }
    }
  }
  return changed;
}

function status() {
  for (const b of BUNDLES) {
    for (const lang of TARGETS) {
      const st = readJson<Record<string, string>>(join(root, b.out, lang, "status.json"), {});
      const v = Object.values(st);
      const reviewed = v.filter((x) => x === "reviewed").length;
      console.log(
        `${b.name.padEnd(10)} ${lang}  ${v.length} strings, ${reviewed} reviewed, ${v.length - reviewed} machine-drafted`,
      );
    }
  }
}

if (import.meta.main) {
  const cmd = process.argv[2];
  if (cmd === "extract") {
    const p = extractAll(true);
    if (p.length) {
      console.error(p.join("\n"));
      process.exit(1);
    }
  } else if (cmd === "check") {
    const p = checkAll();
    if (p.length) {
      console.error(p.slice(0, 200).join("\n"));
      console.error(`\n${p.length} problem(s) in the catalogues`);
      process.exit(1);
    }
    console.log("catalogues ok");
  } else if (cmd === "sync") console.log(`${syncStatus()} status file(s) updated`);
  else if (cmd === "status") status();
  else {
    console.error("usage: i18n.ts extract|check|status|sync");
    process.exit(2);
  }
}
