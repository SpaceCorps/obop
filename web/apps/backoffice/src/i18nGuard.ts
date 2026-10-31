// The reader behind `i18nDeps.test.ts`. It is test support, not part of the app: nothing imports it
// but that test, so the build leaves it out.
//
// The mistake it looks for: text is translated, sorted and formatted by calling the global `t()` and
// the functions built on it while a component renders. The language is not a prop or a state, so a
// `useMemo` that calls any of them keeps the text of the language it was first computed in unless
// the language is one of its dependencies (see `I18nProvider` in @obop/ui).
//
// Which functions are "any of them" is worked out from the source, not listed by hand: a function
// translates when its body (parameters and default values included) refers to one that does, down
// to the few functions of the i18n store that read the active language (`SEEDS`). The graph is
// resolved through imports and re-exports, so `import { label } from "./format.ts"` is that file's
// `label`, not another file's. What is read:
//
//   - a plain name that resolves to a translating function or constant (`appTitle(m)`, and also
//     `.sort(byPosition)`, `.map(shownLabel)`, `fmt.money(1)`: a use, not only a call),
//   - a JSX tag that resolves to one (`<Badge/>`: a cached element does not render again),
//   - a member call whose name is a translating function declared inside another one
//     (`d.recordLabel(...)`, `d.refText(...)`, built in the data provider).
//
// Not read (see ISSUES.md D14): `useCallback`, `useEffect`, an initial `useState(() => t(...))`,
// state that holds a finished translated string, and a translating function reached only through a
// member of an object whose type the reader cannot see.

import { readFileSync, readdirSync } from "node:fs";
import { join, posix } from "node:path";

/** `src` with comments, the text of strings and templates and regular expressions blanked (same length). */
export function code(src: string): string {
  const out = src.split("");
  const n = src.length;
  const blank = (from: number, to: number) => {
    for (let k = from; k < to && k < n; k++) if (out[k] !== "\n") out[k] = " ";
  };
  const templates: number[] = []; // the brace depth at which each open `${` was entered
  let depth = 0;
  let prev = ""; // the last character of code, to tell a division from a regular expression
  let before = ""; // and the one before it (an arrow `=>` is followed by an expression)
  let i = 0;
  // The text of a template literal, from `i` to its closing backtick or the next `${`.
  const templateText = () => {
    const from = i;
    while (i < n) {
      const c = src[i]!;
      if (c === "\\") i += 2;
      else if (c === "`") {
        blank(from, i);
        i++;
        prev = "`";
        return;
      } else if (c === "$" && src[i + 1] === "{") {
        blank(from, i);
        templates.push(depth);
        i += 2;
        prev = "{";
        return;
      } else i++;
    }
    blank(from, n);
  };
  while (i < n) {
    const c = src[i]!;
    const d = src[i + 1];
    if (c === "/" && d === "/") {
      const e = src.indexOf("\n", i);
      const end = e < 0 ? n : e;
      blank(i, end);
      i = end;
    } else if (c === "/" && d === "*") {
      const e = src.indexOf("*/", i + 2);
      const end = e < 0 ? n : e + 2;
      blank(i, end);
      i = end;
    } else if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < n && src[j] !== c && src[j] !== "\n") j += src[j] === "\\" ? 2 : 1;
      blank(i + 1, j);
      i = src[j] === c ? j + 1 : j;
      prev = c;
    } else if (c === "`") {
      i++;
      templateText();
    } else if (
      c === "/" &&
      (prev === "" ||
        "(,=:[!&|?{};".includes(prev) ||
        (prev === ">" && before === "=") ||
        /(?:^|[^\w$])(?:return|typeof|case|in|of|else|do|void|delete|throw|yield|await)\s*$/.test(
          src.slice(Math.max(0, i - 12), i),
        ))
    ) {
      let j = i + 1;
      let inClass = false;
      while (j < n && src[j] !== "\n" && (inClass || src[j] !== "/")) {
        if (src[j] === "\\") j++;
        else if (src[j] === "[") inClass = true;
        else if (src[j] === "]") inClass = false;
        j++;
      }
      if (src[j] === "/") {
        blank(i + 1, j);
        i = j + 1;
        prev = "/";
      } else i++;
    } else {
      if (c === "{") depth++;
      else if (c === "}") {
        if (templates.length && templates[templates.length - 1] === depth) {
          templates.pop();
          i++;
          templateText();
          continue;
        }
        depth--;
      }
      if (!/\s/.test(c)) {
        before = prev;
        prev = c;
      }
      i++;
    }
  }
  return out.join("");
}

/* ---------------------------------------------------------------------------------------------- */
/* Hooks                                                                                           */
/* ---------------------------------------------------------------------------------------------- */

const EXEMPT = /i18n-memo-ok[ \t]*:[ \t]*\S/;
/** What a dependency list must name. */
const LANGUAGE_DEP = /(?<![\w$])(?:lang|locale|i18n)(?![\w$])|\.lang(?![\w$])|\.locale(?![\w$])/;

export type Hook = {
  hook: string;
  line: number;
  /** The function passed to the hook, as written (comments and text blanked). */
  body: string;
  /** The dependency list as written; `null` when the hook has none. */
  deps: string | null;
  exempt: boolean;
  /** Where in the masked source the call starts. */
  at: number;
};

/** Every `useMemo` and `memo` call of a source file. */
export function hooks(src: string, masked: string = code(src)): Hook[] {
  const found: Hook[] = [];
  for (const m of masked.matchAll(/(?<![.\w$])(useMemo|memo)\s*(?=[<(])/g)) {
    let i = m.index! + m[0].length;
    if (masked[i] === "<") {
      // Type arguments: skip to the matching `>`, an arrow's `=>` does not close it.
      let angle = 0;
      for (; i < masked.length; i++) {
        if (masked[i] === "<") angle++;
        else if (masked[i] === ">" && masked[i - 1] !== "=" && --angle === 0) break;
      }
      i++;
      while (/\s/.test(masked[i] ?? "")) i++;
    }
    if (masked[i] !== "(") continue;
    const start = i + 1;
    const args: [number, number][] = [];
    let depth = 0;
    let from = start;
    for (i = start; i < masked.length; i++) {
      const c = masked[i]!;
      if ("([{".includes(c)) depth++;
      else if (")]}".includes(c)) {
        if (depth === 0) break;
        depth--;
      } else if (c === "," && depth === 0) {
        args.push([from, i]);
        from = i + 1;
      }
    }
    if (masked.slice(from, i).trim()) args.push([from, i]);
    const [fn, deps] = args;
    if (!fn) continue;
    const line = src.slice(0, m.index).split("\n").length;
    // The comment may sit on the line before the call or anywhere inside it.
    const lineStart = src.lastIndexOf("\n", m.index! - 1);
    const before = src.slice(src.lastIndexOf("\n", lineStart - 1) + 1, m.index);
    found.push({
      hook: m[1]!,
      line,
      body: masked.slice(fn[0], fn[1]),
      deps: deps ? masked.slice(deps[0], deps[1]).trim() : null,
      exempt: EXEMPT.test(before + src.slice(m.index, i)),
      at: m.index!,
    });
  }
  return found;
}

/** Does the dependency list of a hook name the language or the locale? `null` when it has none. */
export const namesLanguage = (deps: string | null): boolean | null =>
  deps === null ? null : LANGUAGE_DEP.test(deps);

/* ---------------------------------------------------------------------------------------------- */
/* Declarations                                                                                    */
/* ---------------------------------------------------------------------------------------------- */

export type Source = { path: string; text: string };

type Decl = {
  name: string;
  /** Index range in the masked text: the declaration with its parameters and body. */
  start: number;
  end: number;
  /** A function (or a constant that holds one), as against a plain value such as `fmt`. */
  fn: boolean;
};

/** The bracket depth before each character of `masked`, and after the last. */
function depthsOf(masked: string): Int32Array {
  const d = new Int32Array(masked.length + 1);
  let k = 0;
  for (let i = 0; i < masked.length; i++) {
    d[i] = k;
    const c = masked[i];
    if (c === "{" || c === "(" || c === "[") k++;
    else if (c === "}" || c === ")" || c === "]") k--;
  }
  d[masked.length] = k;
  return d;
}

/** The index of the bracket that closes the one at `open`. */
function closer(depth: Int32Array, open: number, n: number): number {
  for (let j = open + 1; j < n; j++) if (depth[j + 1]! === depth[open]!) return j;
  return n - 1;
}

/** The end of the statement that starts at `from`: its `;` at the same depth, or where the block ends. */
function statementEnd(masked: string, depth: Int32Array, from: number): number {
  const base = depth[from]!;
  for (let i = from; i < masked.length; i++) {
    if (masked[i] === ";" && depth[i] === base) return i + 1;
    if (depth[i + 1]! < base) return i;
  }
  return masked.length;
}

/** Declarations of `masked`: `function name` and `const name = ...`, at the top level or inside others. */
function declarations(masked: string, depth: Int32Array, nested: boolean): Decl[] {
  const out: Decl[] = [];
  const n = masked.length;
  const keep = (i: number) => (nested ? depth[i]! > 0 : depth[i]! === 0);
  for (const m of masked.matchAll(/(?<![.\w$])function\s*\*?\s*([A-Za-z_$][\w$]*)\s*(?=[<(])/g)) {
    if (!keep(m.index!)) continue;
    let i = m.index! + m[0].length;
    while (i < n && masked[i] !== "(") i++; // skip type parameters
    const paramsEnd = closer(depth, i, n);
    // The body is the first `{` after the parameters that does not open a type.
    let j = paramsEnd + 1;
    let angle = 0;
    for (; j < n; j++) {
      const c = masked[j]!;
      if (c === "<") angle++;
      else if (c === ">" && masked[j - 1] !== "=") angle--;
      else if (c === "{" && angle === 0) {
        const lead = masked.slice(paramsEnd + 1, j).trimEnd();
        if (!lead.endsWith(":") && !/[|&,<]$/.test(lead)) break;
        j = closer(depth, j, n); // a type literal: `(): { a: string } {`
      } else if (c === ";" && angle === 0) break; // an overload, no body
    }
    out.push({ name: m[1]!, start: m.index!, end: closer(depth, j, n) + 1, fn: true });
  }
  for (const m of masked.matchAll(/(?<![.\w$])(?:const|let|var)\s+([A-Za-z_$][\w$]*)/g)) {
    if (!keep(m.index!)) continue;
    // Skip an annotation (`: Type<...>`) up to the `=` of the declaration.
    let i = m.index! + m[0].length;
    let angle = 0;
    let paren = 0;
    for (; i < n; i++) {
      const c = masked[i]!;
      if (c === "<") angle++;
      else if (c === ">" && masked[i - 1] !== "=") angle--;
      else if (c === "(" || c === "[" || c === "{") paren++;
      else if (c === ")" || c === "]" || c === "}") paren--;
      else if (c === "=" && masked[i + 1] !== ">" && angle <= 0 && paren <= 0) break;
      else if (c === ";" && paren <= 0) break;
    }
    if (masked[i] !== "=") continue; // `const a;` is not valid; a loop variable has no `=` here
    const start = i + 1;
    const init = masked.slice(start, start + 400);
    const fn =
      /^\s*(?:async\s+)?function\b/.test(init) ||
      /^\s*useCallback\s*(?:<[^(]*>)?\s*\(/.test(init) ||
      (/^\s*(?:async\s+)?(?:<[^>(]*>\s*)?(?:\(|[A-Za-z_$][\w$]*\s*=>)/.test(init) &&
        /=>/.test(init.split(";")[0]!));
    out.push({ name: m[1]!, start: m.index!, end: statementEnd(masked, depth, start), fn });
  }
  return out.sort((a, b) => a.start - b.start);
}

/* ---------------------------------------------------------------------------------------------- */
/* Files, imports and exports                                                                      */
/* ---------------------------------------------------------------------------------------------- */

type Binding = { spec: string; name: string };

type File = {
  path: string;
  masked: string;
  depth: Int32Array;
  top: Map<string, Decl>;
  nested: Decl[];
  imports: Map<string, Binding>;
  reexports: Map<string, Binding>;
  stars: string[];
};

/** `import { a, b as c, type D } from "spec"`, default and namespace imports. */
function importsOf(text: string): Map<string, Binding> {
  const out = new Map<string, Binding>();
  const re = /(?:^|\n)\s*import\s+(?:type\s+)?([^;"']*?)\s+from\s*["']([^"']+)["']/g;
  for (const m of text.matchAll(re)) {
    const spec = m[2]!;
    let clause = m[1]!;
    const named = /\{([^}]*)\}/.exec(clause);
    if (named) {
      clause = clause.replace(named[0], "");
      for (const part of named[1]!.split(",")) {
        const p = part.trim().replace(/^type\s+/, "");
        if (!p) continue;
        const [orig, local] = p.split(/\s+as\s+/);
        out.set((local ?? orig)!.trim(), { spec, name: orig!.trim() });
      }
    }
    const ns = /\*\s+as\s+([\w$]+)/.exec(clause);
    if (ns) out.set(ns[1]!, { spec, name: "*" });
    const def = /^\s*([\w$]+)\s*,?\s*$/.exec(clause.replace(/\*\s+as\s+[\w$]+/, ""));
    if (def) out.set(def[1]!, { spec, name: "default" });
  }
  return out;
}

/** `export { a, b as c } from "spec"` and `export * from "spec"`. */
function reexportsOf(text: string): { named: Map<string, Binding>; stars: string[] } {
  const named = new Map<string, Binding>();
  const stars: string[] = [];
  for (const m of text.matchAll(
    /(?:^|\n)\s*export\s+(?:type\s+)?\{([^}]*)\}\s*from\s*["']([^"']+)["']/g,
  ))
    for (const part of m[1]!.split(",")) {
      const p = part.trim().replace(/^type\s+/, "");
      if (!p) continue;
      const [orig, as] = p.split(/\s+as\s+/);
      named.set((as ?? orig)!.trim(), { spec: m[2]!, name: orig!.trim() });
    }
  for (const m of text.matchAll(/(?:^|\n)\s*export\s+\*\s+from\s*["']([^"']+)["']/g))
    stars.push(m[1]!);
  return { named, stars };
}

/** Where `@obop/ui` is: the public entry of the design system, read as source like everything else. */
const UI_ENTRY = "packages/ui/src/index.ts";

function parse(s: Source): File {
  const masked = code(s.text);
  const depth = depthsOf(masked);
  const top = new Map<string, Decl>();
  for (const d of declarations(masked, depth, false)) if (!top.has(d.name)) top.set(d.name, d);
  const re = reexportsOf(s.text);
  return {
    path: s.path,
    masked,
    depth,
    top,
    nested: declarations(masked, depth, true),
    imports: importsOf(s.text),
    reexports: re.named,
    stars: re.stars,
  };
}

/* ---------------------------------------------------------------------------------------------- */
/* The graph                                                                                       */
/* ---------------------------------------------------------------------------------------------- */

/**
 * The functions of the i18n store that read the active language or region directly. Everything else
 * that translates, sorts or formats does so by calling one of these (`t` calls `translate`,
 * `compareText` and every `fmt` formatter call `formatLocale`, `inSentence` calls `getLang`).
 * `classifyStore` makes sure a new reader in the store is not missed.
 */
export const SEEDS = [
  "translate",
  "rawMessage",
  "hasTranslation",
  "getLang",
  "getRegion",
  "getDir",
  "formatLocale",
  "pluralOf",
  "STRING_ENV",
];

/**
 * Functions of the store that read the state without showing it: they apply or save a choice, or
 * resolve it. A memo that calls them does not cache a text.
 */
export const NOT_DISPLAY = ["applyDocument", "initI18n", "resetLanguage"];

const STORE = "packages/ui/src/i18n/store.ts";

/** Top-level functions of the store that read the language, region or catalogue and are in neither list. */
export function unclassifiedReaders(files: Source[]): string[] {
  const store = files.find((f) => f.path === STORE);
  if (!store) return [`${STORE} is not in the sources`];
  const f = parse(store);
  const reads = /\bS\.(?:lang|messages|region)\b(?!\s*=[^=])/;
  const out: string[] = [];
  for (const d of f.top.values())
    if (reads.test(f.masked.slice(d.start, d.end)) && ![...SEEDS, ...NOT_DISPLAY].includes(d.name))
      out.push(d.name);
  return out.sort();
}

export type Analysis = {
  files: Map<string, File>;
  /** `path#name` of every top-level function or constant that translates, sorts or formats. */
  translators: Set<string>;
  /** For each translator, the one it refers to that made it one (a seed has none). */
  via: Map<string, string>;
  /** Names of functions declared inside another that translate and are handed out: `recordLabel`, `refText`. */
  members: Set<string>;
  /** For each top-level declaration (`path#name`), the functions declared inside it that translate. */
  locals: Map<string, Set<string>>;
  /** The nodes that are functions, as against constants such as `fmt`. */
  fns: Set<string>;
};

const resolveSpec = (files: Map<string, File>, from: string, spec: string): string | null => {
  if (spec === "@obop/ui") return UI_ENTRY;
  if (spec === "@obop/ui/static") return "packages/ui/src/static.ts";
  if (!spec.startsWith(".")) return null;
  const base = posix.normalize(posix.join(posix.dirname(from), spec));
  for (const c of [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`])
    if (files.has(c)) return c;
  return null;
};

/** The `path#name` of the top-level declaration that `name` exports from `path`, through re-exports. */
function exported(
  files: Map<string, File>,
  path: string,
  name: string,
  seen = new Set<string>(),
): string | null {
  const key = `${path}#${name}`;
  if (seen.has(key)) return null;
  seen.add(key);
  const f = files.get(path);
  if (!f) return null;
  if (f.top.has(name)) return key;
  const re = f.reexports.get(name);
  if (re) {
    const to = resolveSpec(files, path, re.spec);
    const hit = to && exported(files, to, re.name, seen);
    if (hit) return hit;
  }
  // `import { x } from "./y"; export { x };` and `export default`: follow the import of the same name.
  const imp = f.imports.get(name);
  if (imp && !f.top.has(name)) {
    const to = resolveSpec(files, path, imp.spec);
    const hit = to && imp.name !== "*" && exported(files, to, imp.name, seen);
    if (hit) return hit;
  }
  for (const star of f.stars) {
    const to = resolveSpec(files, path, star);
    const hit = to && exported(files, to, name, seen);
    if (hit) return hit;
  }
  return null;
}

/** The node a plain name refers to in `file`: an import, else a top-level declaration of the file. */
function resolveName(files: Map<string, File>, file: File, name: string): string | null {
  const imp = file.imports.get(name);
  if (imp) {
    const to = resolveSpec(files, file.path, imp.spec);
    return to && imp.name !== "*" ? exported(files, to, imp.name) : null;
  }
  return file.top.has(name) ? `${file.path}#${name}` : null;
}

/** The names a piece of code declares itself: constants, parameters and destructured names. */
function localsOf(masked: string): Set<string> {
  const out = new Set<string>();
  const add = (list: string) => {
    for (const part of list.split(",")) {
      // `a`, `a = 1`, `a: b`, `...a`, `a?: T`: the name that is bound
      const p = part.replace(/^\s*\.\.\./, "").trim();
      const m = /^(?:[A-Za-z_$][\w$]*\s*:\s*)?([A-Za-z_$][\w$]*)/.exec(p);
      if (m && !/^(?:type|typeof)$/.test(m[1]!)) out.add(m[1]!);
    }
  };
  for (const m of masked.matchAll(/(?<![.\w$])(?:const|let|var)\s+([A-Za-z_$][\w$]*)/g))
    out.add(m[1]!);
  for (const m of masked.matchAll(/(?<![.\w$])(?:const|let|var)\s*\{([^}]*)\}/g)) add(m[1]!);
  for (const m of masked.matchAll(/(?<![.\w$])(?:const|let|var)\s*\[([^\]]*)\]/g)) add(m[1]!);
  // parameters: `(a, { b, c }) =>`, `a =>`, `function f(a, b)`
  for (const m of masked.matchAll(/\(([^()]*)\)\s*(?::[^=;{]*?)?=>/g)) {
    for (const p of m[1]!.split(/,(?![^{[]*[}\]])/)) {
      const q = p.trim();
      if (q.startsWith("{") || q.startsWith("[")) add(q.slice(1, -1));
      else add(q);
    }
  }
  for (const m of masked.matchAll(/(?<![\w$])([A-Za-z_$][\w$]*)\s*=>/g)) out.add(m[1]!);
  for (const m of masked.matchAll(/function\s*[\w$]*\s*(?:<[^>(]*>)?\s*\(([^()]*)\)/g)) add(m[1]!);
  return out;
}

export type Refs = {
  /**
   * Resolved `path#name` of every plain name, JSX tag and `fmt`-like object the code uses. The value
   * is false when every use is `name.something`: a function has no members, so that is a variable
   * of the same name (`t.filter(...)`), not the function.
   */
  plain: Map<string, boolean>;
  /** Names used as `x.name(...)` or passed as `(x.name)`. */
  members: Set<string>;
  /** Every plain name, resolved or not: a function declared inside another has no node of its own. */
  bare: Set<string>;
};

/** What `text` (a piece of `file`) refers to. `shadow` names are the code's own and are not imports. */
function refsOf(files: Map<string, File>, file: File, text: string, shadow: Set<string>): Refs {
  const plain = new Map<string, boolean>();
  const members = new Set<string>();
  const bare = new Set<string>();
  const add = (name: string, asObject: boolean) => {
    bare.add(name);
    if (shadow.has(name)) return;
    const key = resolveName(files, file, name);
    if (key) plain.set(key, (plain.get(key) ?? false) || !asObject);
  };
  for (const m of text.matchAll(/(?<![\w$.])([A-Za-z_$][\w$]*)/g)) {
    const name = m[1]!;
    const i = m.index!;
    // `...name` is a spread, `a.name` is a member (the lookbehind skipped it), `{ name: v }` a key
    let b = i - 1;
    while (b >= 0 && /\s/.test(text[b]!)) b--;
    let a = i + name.length;
    while (a < text.length && /\s/.test(text[a]!)) a++;
    if ((text[b] === "{" || text[b] === ",") && text[a] === ":" && text[a + 1] !== ":") continue;
    add(name, text[a] === ".");
  }
  // `...name`: the lookbehind above rejects a name after a dot, which a spread is
  for (const m of text.matchAll(/\.\.\.\s*([A-Za-z_$][\w$]*)/g)) add(m[1]!, false);
  // JSX: `<Name` and `<Name.Part`
  for (const m of text.matchAll(/(?<![\w$])<([A-Z][\w$]*)/g)) add(m[1]!, false);
  // a member call, or a member passed as an argument
  for (const m of text.matchAll(/\.\s*([A-Za-z_$][\w$]*)\s*\(/g)) members.add(m[1]!);
  for (const m of text.matchAll(/[(,]\s*[\w$.]+\.([A-Za-z_$][\w$]*)\s*(?=[,)])/g))
    members.add(m[1]!);
  return { plain, members, bare };
}

/** Names that a Map, a Set, an array or a string already has: a function of that name is not a member to follow. */
const BUILT_IN = new Set(
  "set get add has delete clear keys values entries map filter find some every sort slice splice join push pop shift reverse concat includes indexOf forEach reduce flat flatMap test exec replace match split trim then catch finally call apply bind toString".split(
    " ",
  ),
);

/**
 * Is a function declared inside another handed out as a property of an object whose type declares
 * a function of that name (`recordLabel: (model: string, row: Row) => string` in the data context)?
 */
function exposed(file: File, d: Decl, typed: Set<string>): boolean {
  if (BUILT_IN.has(d.name) || !typed.has(d.name)) return false;
  const rest = file.masked.slice(0, d.start) + file.masked.slice(d.end);
  return new RegExp(String.raw`[{,]\s*${d.name}\s*(?=[,}:])`).test(rest);
}

/** Parse every file and work out which functions translate, sort or format. */
export function analyse(sources: Source[]): Analysis {
  const files = new Map<string, File>();
  for (const s of sources) files.set(s.path, parse(s));
  // names that some type or object gives as a function: `name: (...) => ...`
  const typed = new Set<string>();
  for (const f of files.values())
    for (const m of f.masked.matchAll(
      /(?<![.\w$])([A-Za-z_$][\w$]*)\??\s*:\s*(?:async\s+)?\([^()]*\)\s*(?::[^=;]*)?=>/g,
    ))
      typed.add(m[1]!);

  type Node = { key: string; file: File; decl: Decl; refs: Refs; seed: boolean };
  const nodes: Node[] = [];
  const own = (file: File, d: Decl) => {
    // a function does not shadow itself, and a declaration's parameters are its own
    const text = file.masked.slice(d.start, d.end);
    const shadow = localsOf(text);
    shadow.delete(d.name);
    return { text, shadow };
  };
  for (const file of files.values())
    for (const d of file.top.values()) {
      const { text, shadow } = own(file, d);
      nodes.push({
        key: `${file.path}#${d.name}`,
        file,
        decl: d,
        refs: refsOf(files, file, text, shadow),
        seed: file.path === STORE && SEEDS.includes(d.name),
      });
    }

  // Functions declared inside others. Each is a local of the one around it (`label` in a component),
  // and one that is handed out as a property is a member for `d.name(...)` anywhere.
  const inner: { scope: string; file: File; decl: Decl; refs: Refs }[] = [];
  for (const file of files.values())
    for (const d of file.nested) {
      if (!d.fn) continue;
      const outer = [...file.top.values()].find((t) => t.start <= d.start && d.start < t.end);
      if (!outer) continue;
      const { text, shadow } = own(file, d);
      inner.push({
        scope: `${file.path}#${outer.name}`,
        file,
        decl: d,
        refs: refsOf(files, file, text, shadow),
      });
    }

  const isFn = new Map(nodes.map((n) => [n.key, n.decl.fn]));
  const translators = new Set<string>(nodes.filter((n) => n.seed).map((n) => n.key));
  const via = new Map<string, string>();
  const members = new Set<string>();
  const locals = new Map<string, Set<string>>(); // scope -> names of its translating inner functions
  for (let grown = true; grown;) {
    grown = false;
    const reaches = (r: Refs, scope?: string): string | null => {
      for (const [k, direct] of r.plain)
        if (translators.has(k) && (direct || !isFn.get(k))) return k.split("#")[1]!;
      for (const m of r.members) if (members.has(m)) return `.${m}`;
      if (scope) for (const b of r.bare) if (locals.get(scope)?.has(b)) return b;
      return null;
    };
    for (const n of nodes) {
      const why = translators.has(n.key) ? null : reaches(n.refs, n.key);
      if (why) {
        translators.add(n.key);
        via.set(n.key, why);
        grown = true;
      }
    }
    for (const x of inner) {
      const mine = locals.get(x.scope) ?? locals.set(x.scope, new Set()).get(x.scope)!;
      if (mine.has(x.decl.name) || !reaches(x.refs, x.scope)) continue;
      mine.add(x.decl.name);
      grown = true;
    }
    for (const x of inner)
      if (
        !members.has(x.decl.name) &&
        locals.get(x.scope)?.has(x.decl.name) &&
        exposed(x.file, x.decl, typed)
      ) {
        members.add(x.decl.name);
        grown = true;
      }
  }
  return {
    files,
    translators,
    via,
    members,
    locals,
    fns: new Set([...isFn].filter(([, f]) => f).map(([k]) => k)),
  };
}

/** What a hook uses that translates, sorts or formats; empty when nothing. */
export function usesTranslators(a: Analysis, path: string, hook: Hook): string[] {
  const file = a.files.get(path);
  if (!file) return [];
  // the names of the component around the hook are its own: constants and parameters declared there
  let outer = new Set<string>();
  let scope = "";
  for (const d of file.top.values())
    if (d.start <= hook.at && hook.at < d.end) {
      outer = localsOf(file.masked.slice(d.start, d.end));
      scope = `${path}#${d.name}`;
    }
  const shadow = new Set([...outer, ...localsOf(hook.body)]);
  const refs = refsOf(a.files, file, hook.body, shadow);
  const hit: string[] = [];
  for (const [key, direct] of refs.plain)
    if (a.translators.has(key) && (direct || !a.fns.has(key))) hit.push(key.split("#")[1]!);
  for (const m of refs.members) if (a.members.has(m)) hit.push(`.${m}`);
  // a helper of the component around the hook that translates (`const label = (x) => t(x)`)
  for (const b of refs.bare) if (a.locals.get(scope)?.has(b)) hit.push(b);
  return [...new Set(hit)].sort();
}

/** Why a hook can keep an old language, or `null` when it cannot. */
export function problem(a: Analysis, path: string, h: Hook): string | null {
  if (h.exempt) return null;
  const uses = usesTranslators(a, path, h);
  if (!uses.length) return null;
  if (h.hook === "memo")
    return /(?<![.\w$])useI18n\s*\(/.test(h.body)
      ? null
      : `a memo component that uses ${uses.join(", ")} does not use useI18n()`;
  if (h.deps === null) return null; // computed on every render
  if (!h.deps.startsWith("[")) return "the dependencies are not a list the guard can read";
  return namesLanguage(h.deps)
    ? null
    : `uses ${uses.join(", ")}; the dependencies do not name lang or locale`;
}

/* ---------------------------------------------------------------------------------------------- */
/* The tree                                                                                        */
/* ---------------------------------------------------------------------------------------------- */

/** Every source file of the apps and of the design system, paths relative to `web/`. */
export function readTree(web: string): Source[] {
  const roots = [
    ...readdirSync(join(web, "apps")).map((a) => join("apps", a, "src")),
    "packages/ui/src",
  ];
  const out: Source[] = [];
  const walk = (dir: string) => {
    let entries;
    try {
      entries = readdirSync(join(web, dir), { withFileTypes: true });
    } catch {
      return; // an app without a src directory
    }
    for (const e of entries) {
      const p = posix.join(dir.replaceAll("\\", "/"), e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.tsx?$/.test(e.name) && !/\.test\.tsx?$/.test(e.name))
        out.push({ path: p, text: readFileSync(join(web, p), "utf8") });
    }
  };
  roots.forEach(walk);
  return out.sort((a, b) => a.path.localeCompare(b.path));
}
