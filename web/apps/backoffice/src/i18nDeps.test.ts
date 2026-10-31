// A guard for one mistake that has already cost a bug: text is translated and sorted by calling the
// global `t()` and friends while a component renders; the language is not a prop or a state, so a
// `useMemo` that calls them keeps the text of the language it was first computed in unless the
// language is one of its dependencies (see the comment of `I18nProvider` in @obop/ui). This test
// reads the source, finds every `useMemo` and `memo` and fails when the function it is given calls
// one of the translating functions below and the dependencies name neither `lang` nor `locale`
// (`d.lang` of the data context and `useI18n()`'s `i18n` count).
//
// `useCallback` is not read: its function runs when it is called, so a `t()` in it reads the language
// then, and a cached function cannot hold an old text. (Reading it would flag five more hooks, every
// one of them lazy in this way: a confirm dialog's default texts, an error message, `nameOf`.) The
// same goes for a function inside a memo that the memo only returns, for example the handlers of a
// card: mark that memo as below.
//
// A hook that is right although it looks wrong is marked in the source, on the line before it or
// inside it, with a comment that says why:
//
//   // i18n-memo-ok: the callback runs when the button is pressed, so `t()` is read then
//
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vite-plus/test";

/**
 * Functions whose result depends on the language or the region: `t` and `tx`, `rich`, the label
 * helpers of the back office (labels.ts, format.ts), the collation (`compareText`) and the number,
 * money and date formatters of `fmt`.
 */
export const TRANSLATING = [
  "t",
  "tx",
  "rich",
  "appTitle",
  "modelTitle",
  "fieldTitle",
  "fieldTitleByName",
  "actionTitle",
  "optionLabel",
  "shownLabel",
  "describeFor",
  "labelFor",
  "compareText",
  // Helpers of the back office that call the above: the list of apps, built from the modules.
  "buildApps",
  "appBlurb",
];
/** A call of one of the `names`, or of a formatter of `fmt`. */
const callsOf = (names: string[]) =>
  new RegExp(`(?<![.\\w$])(?:${names.join("|")})\\s*\\(|(?<![.\\w$])fmt(?:Locale)?\\s*\\.\\s*\\w+`);
/** What a dependency list must name. */
const LANGUAGE_DEP = /(?<![\w$])(?:lang|locale|i18n)(?![\w$])|\.lang(?![\w$])|\.locale(?![\w$])/;
const EXEMPT = /i18n-memo-ok[ \t]*:[ \t]*\S/;

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
      (prev === "" || "(,=:[!&|?{};".includes(prev) || (prev === ">" && before === "="))
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

export type Hook = {
  hook: string;
  line: number;
  /** The function passed to the hook, as written. */
  body: string;
  /** The dependency list as written; `null` when the hook has none. */
  deps: string | null;
  exempt: boolean;
};

/** Every `useMemo` and `memo` call of a source file. */
export function hooks(src: string): Hook[] {
  const masked = code(src);
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
    });
  }
  return found;
}

/** Why a hook can keep an old language, or `null` when it cannot. */
export function problem(h: Hook, names: string[] = TRANSLATING): string | null {
  if (h.exempt || !callsOf(names).test(h.body)) return null;
  if (h.hook === "memo")
    return /(?<![.\w$])useI18n\s*\(/.test(h.body)
      ? null
      : "a memo component translates but does not use useI18n()";
  if (h.deps === null) return null; // computed on every render
  if (!h.deps.startsWith("[")) return "the dependencies are not a list the guard can read";
  return LANGUAGE_DEP.test(h.deps) ? null : "the dependencies do not name lang or locale";
}

describe("the guard reads hooks", () => {
  it("flags a memo that translates without the language in its dependencies", () => {
    const [h] = hooks(`
      const a = useMemo(() => buildApps(m).map((x) => t("Open {name}", { name: x })), [m]);`);
    expect(problem(h!)).toMatch(/do not name lang or locale/);
  });

  it("accepts the language or the locale among the dependencies, in any spelling", () => {
    for (const deps of ["[m, lang]", "[m, d.lang]", "[locale, m]", "[m, i18n]", "[m, i18n.lang]"])
      expect(problem(hooks(`useMemo(() => compareText(a, b), ${deps})`)[0]!), deps).toBeNull();
  });

  it("looks at every function that translates or sorts", () => {
    for (const call of [
      "appTitle(m)",
      'modelTitle(m, "x")',
      "fieldTitle(f)",
      'tx("app", n)',
      "compareText(a, b)",
      "fmt.money(1)",
    ])
      expect(problem(hooks(`useMemo(() => ${call}, [m])`)[0]!), call).not.toBeNull();
  });

  it("handles type arguments, nested brackets, arrow functions in the arguments and a second hook", () => {
    const hs = hooks(`
      const a = useMemo<Hit[]>(() => rows.map((r) => ({ k: [t("x"), 1], f: (u: Map<string, number>) => u })), [rows, q]);
      const b = useMemo(async (x = t("y")) => x, []);`);
    expect(hs.map((h) => [h.hook, h.deps])).toEqual([
      ["useMemo", "[rows, q]"],
      ["useMemo", "[]"],
    ]);
    expect(hs.map((h) => problem(h) !== null)).toEqual([true, true]);
  });

  it("ignores what is only text: comments, strings, templates and regular expressions", () => {
    const src = [
      "// useMemo(() => t('a'), [])",
      "/* useMemo(() => t('a'), []) */",
      "const s = \"useMemo(() => t('a'), [])\";",
      "const r = /useMemo\\(\\(\\) => t\\('a'\\)/;",
      "const q = `${useMemo(() => 1, [])} useMemo(() => t('a'), [])`;",
      "const ok = useMemo(() => /['\"]/.test(x) && `a${y}b`, [x, y]);",
    ].join("\n");
    // What is code stays (`${y}`), what is text is blank.
    expect(hooks(src).map((h) => h.body.trim())).toEqual([
      "() => 1",
      "() => /    /.test(x) && ` ${y} `",
    ]);
    expect(hooks(src).map((h) => problem(h))).toEqual([null, null]);
  });

  it("does not take a call of something else for a translation", () => {
    for (const body of ["() => item.t(1)", "() => format(1)", "() => split(a)", "() => 'tx(' + a"])
      expect(problem(hooks(`useMemo(${body}, [])`)[0]!), body).toBeNull();
  });

  it("lets a hook that is right say so, with a reason", () => {
    const withReason = "// i18n-memo-ok: runs when pressed\nconst f = useMemo(() => t('x'), []);";
    expect(problem(hooks(withReason)[0]!)).toBeNull();
    const inside =
      "const f = useMemo(\n  // i18n-memo-ok: the text is built when the row is drawn\n  () => t('x'),\n  [],\n);";
    expect(problem(hooks(inside)[0]!)).toBeNull();
    // A bare marker is not a reason.
    expect(
      problem(hooks("// i18n-memo-ok:\nconst f = useMemo(() => t('x'), []);")[0]!),
    ).not.toBeNull();
  });

  it("holds a memo component to using the language hook", () => {
    expect(problem(hooks("const C = memo(function C() { return t('x'); });")[0]!)).not.toBeNull();
    expect(
      problem(hooks("const C = memo(function C() { useI18n(); return t('x'); });")[0]!),
    ).toBeNull();
  });
});

/** Every source file of the apps and of the design system that the back office is made of. */
function sources(): string[] {
  const here = dirname(fileURLToPath(import.meta.url));
  const web = join(here, "../../..");
  const roots = [
    ...readdirSync(join(web, "apps")).map((a) => join(web, "apps", a, "src")),
    join(web, "packages/ui/src"),
  ];
  const files: string[] = [];
  const walk = (dir: string) => {
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return; // an app without a src directory
    }
    for (const e of entries) {
      const p = join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.tsx?$/.test(e.name) && !/\.test\.tsx?$/.test(e.name)) files.push(p);
    }
  };
  roots.forEach(walk);
  return files.sort();
}

describe("the source of the apps", () => {
  it("has no memo that translates or sorts without the language in its dependencies", () => {
    const web = join(dirname(fileURLToPath(import.meta.url)), "../../..");
    const files = sources().map((file) => ({ file, src: readFileSync(file, "utf8") }));
    const flagged: string[] = [];
    let seen = 0;
    for (const { file, src } of files) {
      const hs = hooks(src);
      seen += hs.length;
      for (const h of hs) {
        const why = problem(h);
        if (why)
          flagged.push(`${relative(web, file).replaceAll("\\", "/")}:${h.line} ${h.hook}: ${why}`);
      }
    }
    // The scan must have found the hooks (a broken reader would find none and pass).
    expect(seen).toBeGreaterThan(40);
    expect(
      flagged,
      "Add lang or locale to the dependencies, or mark the hook with i18n-memo-ok: <why>",
    ).toEqual([]);
  });
});
