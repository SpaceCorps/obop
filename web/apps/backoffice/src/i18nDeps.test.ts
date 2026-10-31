// A guard for one mistake that has already cost a bug (the app list, the smart buttons, the dashboards):
// text is translated, sorted and formatted by calling the global `t()` and the functions built on it
// while a component renders. The language is not a prop or a state, so a `useMemo` that uses any of
// them keeps the text of the language it was first computed in unless the language is one of its
// dependencies (see `I18nProvider` in @obop/ui).
//
// `i18nGuard.ts` reads the source of every app and of the design system, works out from the call
// graph which functions translate, sort or format, and this test fails when a `useMemo` (or a `memo`
// component) uses one of them and its dependencies name neither `lang` nor `locale` (`d.lang` of the
// data context and `useI18n()`'s `i18n` count). What it reads and what it does not is at the top of
// that file; ISSUES.md D14 lists the blind spots.
//
// A hook that is right although it looks wrong is marked in the source, on the line before it or
// inside it, with a comment that says why:
//
//   // i18n-memo-ok: the callback runs when the button is pressed, so `t()` is read then
//
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vite-plus/test";
import {
  SEEDS,
  analyse,
  code,
  hooks,
  namesLanguage,
  problem,
  readTree,
  unclassifiedReaders,
  type Source,
} from "./i18nGuard.ts";

/* ---------------------------------------------------------------------------------------------- */
/* Fixtures: a small design system and some apps, as the reader sees them                          */
/* ---------------------------------------------------------------------------------------------- */

const UI: Record<string, string> = {
  "packages/ui/src/index.ts": 'export * from "./i18n/index.ts";',
  "packages/ui/src/i18n/index.ts":
    'export { t, fmt, compareText, getLang, setLanguage } from "./store.ts";',
  "packages/ui/src/i18n/store.ts": `
    const S = { lang: "en", messages: {} as Record<string, string>, region: "auto" };
    export function translate(id: string) { return S.messages[id] ?? id; }
    export function t(id: string) { return translate(id); }
    export const getLang = () => S.lang;
    export function formatLocale() { return S.region === "auto" ? S.lang : S.region; }
    export const fmt = { money: (n: number) => String(n) + formatLocale() };
    export function compareText(a: string, b: string) { return a.localeCompare(b, formatLocale()); }
    export async function setLanguage(lang: string) { S.lang = lang; }
  `,
};

const sources = (apps: Record<string, string>): Source[] =>
  Object.entries({ ...UI, ...apps }).map(([path, text]) => ({ path, text }));

/** The problems of every memo in `apps`, as `file:line text`. */
function flagged(apps: Record<string, string>): string[] {
  const all = sources(apps);
  const a = analyse(all);
  const out: string[] = [];
  for (const s of all)
    for (const h of hooks(s.text, a.files.get(s.path)!.masked)) {
      const why = problem(a, s.path, h);
      if (why) out.push(`${s.path}:${h.line} ${why}`);
    }
  return out;
}

const web = join(dirname(fileURLToPath(import.meta.url)), "../../..");

/** Brackets left open or closed too often in `src` once text is blanked: a mistake of the reader. */
function imbalance(src: string): number {
  let depth = 0;
  for (const c of code(src)) depth += "{([".includes(c) ? 1 : "})]".includes(c) ? -1 : 0;
  return depth;
}

/* ---------------------------------------------------------------------------------------------- */
/* The reader                                                                                      */
/* ---------------------------------------------------------------------------------------------- */

describe("the reader of hooks", () => {
  it("finds the dependencies of a memo", () => {
    const [h] = hooks(`
      const a = useMemo(() => rows.map((x) => t("Open {name}", { name: x })), [m]);`);
    expect(h!.deps).toBe("[m]");
    expect(namesLanguage(h!.deps)).toBe(false);
  });

  it("accepts the language or the locale among the dependencies, in any spelling", () => {
    for (const deps of ["[m, lang]", "[m, d.lang]", "[locale, m]", "[m, i18n]", "[m, i18n.lang]"])
      expect(namesLanguage(hooks(`useMemo(() => 1, ${deps})`)[0]!.deps), deps).toBe(true);
    for (const deps of ["[m, language]", "[m, d.version]", "[m, lang2]"])
      expect(namesLanguage(hooks(`useMemo(() => 1, ${deps})`)[0]!.deps), deps).toBe(false);
  });

  it("handles type arguments, nested brackets, arrow functions in the arguments and a second hook", () => {
    const hs = hooks(`
      const a = useMemo<Hit[]>(() => rows.map((r) => ({ k: [t("x"), 1], f: (u: Map<string, number>) => u })), [rows, q]);
      const b = useMemo(async (x = t("y")) => x, []);`);
    expect(hs.map((h) => [h.hook, h.deps])).toEqual([
      ["useMemo", "[rows, q]"],
      ["useMemo", "[]"],
    ]);
  });

  it("ignores what is only text: comments, strings, templates and regular expressions", () => {
    const src = [
      "// useMemo(() => t('a'), [])",
      "/* useMemo(() => t('a'), []) */",
      "const s = \"useMemo(() => t('a'), [])\";",
      "const r = /useMemo\\(\\(\\) => t\\('a'\\)/;",
      "const q = `${useMemo(() => 1, [])} useMemo(() => t('a'), [])`;",
      "const ok = useMemo(() => /['\"]/.test(x) && `a${y}b`, [x, y]);",
      'function csv(s) { return /[",\\n]/.test(s) ? `"${s.replace(/"/g, \'""\')}"` : s; }',
    ].join("\n");
    // What is code stays (`${y}`), what is text is blank.
    expect(hooks(src).map((h) => h.body.trim())).toEqual([
      "() => 1",
      "() => /    /.test(x) && ` ${y} `",
    ]);
    // and the brackets of what is left balance, which they do not when a string or a regular
    // expression is mistaken for code
    expect(imbalance(src)).toBe(0);
  });

  it("lets a hook that is right say so, with a reason", () => {
    const app = (marker: string) => ({
      "apps/x/src/a.tsx": `import { t } from "@obop/ui";\n${marker}\nconst f = useMemo(() => t("x"), []);`,
    });
    expect(flagged(app("// i18n-memo-ok: runs when pressed"))).toEqual([]);
    expect(
      flagged({
        "apps/x/src/a.tsx":
          'import { t } from "@obop/ui";\nconst f = useMemo(\n  // i18n-memo-ok: the text is built when the row is drawn\n  () => t("x"),\n  [],\n);',
      }),
    ).toEqual([]);
    // A bare marker is not a reason, and neither is a dash.
    expect(flagged(app("// i18n-memo-ok:"))).toHaveLength(1);
    expect(flagged(app("// i18n-memo-ok: -"))).toHaveLength(1);
    expect(flagged(app("// i18n-memo-ok: ::"))).toHaveLength(1);
    // A block comment is a comment too.
    expect(flagged(app("/* i18n-memo-ok: runs when pressed */"))).toEqual([]);
  });

  it("marks the statement under the comment, not the one after it", () => {
    const at = (first: string) =>
      flagged({
        "apps/x/src/a.tsx": `import { t } from "@obop/ui";\n${first}\nconst b = useMemo(() => t("b"), []);`,
      }).map((x) => x.split(" ")[0]);
    // A comment after a hook on its line is that hook's trailing comment: it is not a marker above
    // the hook, and it must not reach the hook on the next line either.
    expect(at('const a = useMemo(() => t("a"), []); // i18n-memo-ok: pressed later')).toEqual([
      "apps/x/src/a.tsx:2",
      "apps/x/src/a.tsx:3",
    ]);
    // The marker above a statement of several lines counts for the hook in it, and for it alone.
    expect(at('// i18n-memo-ok: pressed later\nconst a =\n  useMemo(() => t("a"), []);')).toEqual([
      "apps/x/src/a.tsx:5",
    ]);
    // A blank line, or code, between the comment and the statement breaks the link.
    expect(at('// i18n-memo-ok: pressed later\n\nconst a = useMemo(() => t("a"), []);')).toEqual([
      "apps/x/src/a.tsx:4",
      "apps/x/src/a.tsx:5",
    ]);
  });

  it("reads the dependencies of a memo whose function has type arguments with a comma", () => {
    const [h] = hooks(
      "const v = useMemo(() => new Map<string, string>(ids.map((i) => [i, t(i)])), [ids]);",
    );
    expect(h!.deps).toBe("[ids]");
    expect(h!.body).toContain("t(i)");
    // no dependencies at all, with the same comma in the type arguments
    expect(hooks("const v = useMemo(() => new Map<string, string>());")[0]!.deps).toBeNull();
    // and the trailing comma a formatter writes after the dependencies of a call over several lines
    const [m] = hooks("const v = useMemo(\n  () => new Map<string, string>(),\n  [a, b],\n);");
    expect(m!.deps).toBe("[a, b]");
    expect(m!.body.trim()).toBe("() => new Map<string, string>()");
  });
});

describe("what counts as translating", () => {
  const f = (body: string, extra: Record<string, string> = {}) =>
    flagged({
      "apps/x/src/a.tsx": `import { t, fmt, compareText, getLang, setLanguage } from "@obop/ui";\n${body}`,
      ...extra,
    });

  it("is a call of t, a formatter of fmt, the collation or the language itself", () => {
    for (const use of ['t("x")', "fmt.money(1)", "compareText(a, b)", "getLang()"])
      expect(f(`const v = useMemo(() => ${use}, [m]);`), use).toHaveLength(1);
    // with the language in the dependencies it is fine
    expect(f('const v = useMemo(() => t("x"), [m, lang]);')).toEqual([]);
    expect(f("const v = useMemo(() => fmt.money(1), [m, d.locale]);")).toEqual([]);
  });

  it("is a function that calls one of those, however far, in the same file or another", () => {
    const apps = {
      "apps/x/src/lib.ts": `
        import { t } from "@obop/ui";
        export const label = (id: string) => t(id);
        export function title(m: { name: string }) { return label(m.name); }
        export const plain = (a: number) => a + 1;`,
      "apps/x/src/mid.ts": `
        import { title as nameOf } from "./lib.ts";
        export const heading = (m: { name: string }) => nameOf(m).toUpperCase();`,
      "apps/x/src/barrel.ts": `export { heading as top } from "./mid.ts";\nexport * from "./lib.ts";`,
    };
    for (const [use, from] of [
      ["title(m)", `import { title } from "./lib.ts";`],
      ["heading(m)", `import { heading } from "./mid.ts";`],
      ["top(m)", `import { top } from "./barrel.ts";`],
      ["label(m)", `import { label } from "./barrel.ts";`],
      ["alias(m)", `import { title as alias } from "./lib.ts";`],
    ] as const)
      expect(
        flagged({ ...apps, "apps/x/src/a.tsx": `${from}\nconst v = useMemo(() => ${use}, [m]);` }),
        use,
      ).toHaveLength(1);
    // a function that does not translate is not flagged, nor one of the same name in another file
    expect(
      flagged({
        ...apps,
        "apps/x/src/a.tsx": `import { plain } from "./lib.ts";\nconst v = useMemo(() => plain(1), [m]);`,
      }),
    ).toEqual([]);
    expect(
      flagged({
        ...apps,
        "apps/x/src/other.ts": "export const title = (m: string) => m;",
        "apps/x/src/a.tsx": `import { title } from "./other.ts";\nconst v = useMemo(() => title("a"), [m]);`,
      }),
    ).toEqual([]);
  });

  it("is a function that is used and not called: .sort(byPosition), .map(label)", () => {
    const lib = `const byPosition = (a: { n: string }, b: { n: string }) => compareText(a.n, b.n);
      const label = (id: string) => t(id);`;
    expect(f(`${lib}\nconst v = useMemo(() => rows.sort(byPosition), [rows]);`)).toHaveLength(1);
    expect(f(`${lib}\nconst v = useMemo(() => ids.map(label), [ids]);`)).toHaveLength(1);
    expect(
      f(`${lib}\nconst v = useMemo(() => [...rows].sort(byPosition), [rows, locale]);`),
    ).toEqual([]);
  });

  it("is a function whose default value calls one: describe(o, lang = getLang())", () => {
    const lib = "const describe = (o: { d: string }, lang = getLang()) => o.d + lang;";
    expect(f(`${lib}\nconst v = useMemo(() => describe(o), [o]);`)).toHaveLength(1);
  });

  it("is a component in a cached element, which does not render again", () => {
    const lib = 'const Badge = ({ n }: { n: number }) => <b>{t("count")}{n}</b>;';
    expect(f(`${lib}\nconst v = useMemo(() => <Badge n={1} />, []);`)).toHaveLength(1);
    expect(
      f(`${lib}\nconst v = useMemo(() => rows.map((r) => <Badge key={r} n={r} />), [rows]);`),
    ).toHaveLength(1);
  });

  it("is a member that the data context builds from a translating function: d.recordLabel, d.refText", () => {
    const apps = {
      "apps/x/src/data.tsx": `
        import { t } from "@obop/ui";
        export type Ctx = { recordLabel: (m: string) => string; refText: (m: string) => string };
        export const labelOf = (m: string) => t(m);
        export function provide(): Ctx {
          const recordLabel = (m: string) => labelOf(m);
          const refText = (m: string) => recordLabel(m);
          return { recordLabel, refText };
        }`,
    };
    for (const use of [
      "d.recordLabel(m)",
      "d.refText(m)",
      "rows.map(d.recordLabel)",
      "pick(d.refText, 1)",
    ])
      expect(
        flagged({ ...apps, "apps/x/src/a.tsx": `const v = useMemo(() => ${use}, [rows]);` }),
        use,
      ).toHaveLength(1);
    // another method that is not handed out by a translating function is left alone
    expect(
      flagged({ ...apps, "apps/x/src/a.tsx": "const v = useMemo(() => d.version(m), [rows]);" }),
    ).toEqual([]);
  });

  it("follows a member on the data context and on a lookup, not on any receiver", () => {
    const apps = {
      "apps/x/src/data.tsx": `
        import { t } from "@obop/ui";
        export type Ctx = { refText: (m: string) => string; update: (m: string) => string };
        export const labelOf = (m: string) => t(m);
        export function provide(): Ctx {
          const refText = (m: string) => labelOf(m);
          const update = (m: string) => labelOf(m);
          return { refText, update };
        }`,
    };
    const use = (body: string) =>
      flagged({ ...apps, "apps/x/src/a.tsx": `const v = useMemo(() => ${body}, [rows]);` });
    expect(use("d.update(m)")).toHaveLength(1);
    expect(use("lk.refText(m)")).toHaveLength(1);
    // `update` and `create` are names other objects have too: a hash, a map, a Date
    expect(use("hash.update(m)")).toEqual([]);
    expect(use("Object.create(null)")).toEqual([]);
  });

  it("reads a memo whose function has type arguments with a comma", () => {
    const memo = (deps: string) =>
      f(`const v = useMemo(() => new Map<string, string>(ids.map((i) => [i, t(i)])), ${deps});`);
    expect(memo("[ids]")).toHaveLength(1);
    expect(memo("[ids, lang]")).toEqual([]);
    // no dependencies: computed on every render, so nothing to flag and nothing unreadable
    expect(f("const v = useMemo(() => new Map<string, string>([[t(a), 1]]));")).toEqual([]);
  });

  it("is a helper whose parameters or result are typed with an object", () => {
    // a `;` inside the object type must not cut the declaration short
    expect(
      f(`function C() {
        const label = (x: { a: string; b: number }) => t(x.a);
        const v = useMemo(() => ids.map((i) => label(i)), [ids]);
        return v;
      }`),
    ).toHaveLength(1);
    expect(
      f(`function C() {
        const label = (x: string): { a: string; b: number } => ({ a: t(x), b: 1 });
        const v = useMemo(() => ids.map((i) => label(i)), [ids]);
        return v;
      }`),
    ).toHaveLength(1);
    expect(
      f(`function C() {
        const label = <T extends { p?: number; title: string }>(x: T) => t(x.title);
        const v = useMemo(() => ids.map((i) => label(i)), [ids]);
        return v;
      }`),
    ).toHaveLength(1);
    // a constant that only holds an object with an arrow in it is not a function
    expect(
      f(`function C() {
        const table = (1 + 2) * ids.length + ((x: number) => x)(1) * 0 + (ids.map((i) => t(i)).length);
        const v = useMemo(() => table, [ids]);
        return v;
      }`),
    ).toEqual([]);
  });

  it("shadows only what is in scope at the memo, not every callback of the component", () => {
    // a callback of the component names a parameter `t`; the memo's `t` is still the import
    expect(
      f(`function C() {
        const [rows, setRows] = useState([]);
        const add = () => setRows((t) => [...t, 1]);
        const v = useMemo(() => t("Open"), [rows]);
        return [add, v];
      }`),
    ).toHaveLength(1);
    // so is one below it, a loop variable, and a typed parameter of another function
    expect(
      f(`function C() {
        const v = useMemo(() => t("Open"), [rows]);
        const w = (t: number) => t + 1;
        for (const t of rows) { void t; }
        return [v, w];
      }`),
    ).toHaveLength(1);
    // what is in scope shadows: a constant above, a parameter, a destructured property
    expect(
      f(`function C() { const t = (s: string) => s; return useMemo(() => t("a"), []); }`),
    ).toEqual([]);
    expect(
      f(`const C = (t: (s: string) => string) => { return useMemo(() => t("a"), []); };`),
    ).toEqual([]);
    expect(
      f(
        `function C({ appTitle: t }: { appTitle: (s: string) => string }) { return useMemo(() => t("a"), []); }`,
      ),
    ).toEqual([]);
    expect(
      f(`function C({ compareText }: Props) { return useMemo(() => compareText(1), []); }`),
    ).toEqual([]);
    // a name of the memo's own callback shadows inside the memo
    expect(f(`function C() { return useMemo(() => rows.map((t) => t.id), [rows]); }`)).toEqual([]);
  });

  it("is a helper of the component around the memo", () => {
    expect(
      f(`function C() {
        const label = (id: string) => t(id);
        const v = useMemo(() => ids.map((i) => label(i)), [ids]);
        return v;
      }`),
    ).toHaveLength(1);
  });

  it("does not mistake a name of its own for the function: t as a variable, a local compareText", () => {
    expect(
      f("const v = useMemo(() => rows.filter((t) => t.ok).map((t) => t.id), [rows]);"),
    ).toEqual([]);
    expect(
      f(`function C() {
        const compareText = 3;
        const v = useMemo(() => compareText + 1, []);
        return v;
      }`),
    ).toEqual([]);
    // and a key of an object literal is not a use
    expect(f("const v = useMemo(() => ({ t: 1, fmt: 2, compareText: 3 }), []);")).toEqual([]);
  });

  it("holds a memo component to using the language hook", () => {
    expect(f('const C = memo(function C() { return t("x"); });')).toHaveLength(1);
    expect(f('const C = memo(function C() { useI18n(); return t("x"); });')).toEqual([]);
  });

  it("does not follow what only changes the language", () => {
    expect(f('const v = useMemo(() => setLanguage("de"), []);')).toEqual([]);
  });
});

/* ---------------------------------------------------------------------------------------------- */
/* The source of the apps                                                                          */
/* ---------------------------------------------------------------------------------------------- */

describe("the source of the apps", () => {
  const tree = readTree(web);
  const a = analyse(tree);
  const names = new Set([...a.translators].map((k) => k.split("#")[1]!));

  it("reads every file, and the brackets of each balance once text is blanked", () => {
    expect(tree.length).toBeGreaterThan(100);
    expect(tree.filter((s) => imbalance(s.text) !== 0).map((s) => s.path)).toEqual([]);
  });

  it("knows every function of the i18n store that reads the language", () => {
    // a new reader must be named in SEEDS (it shows a text) or NOT_DISPLAY (it applies a choice)
    expect(unclassifiedReaders(tree)).toEqual([]);
    for (const seed of SEEDS)
      expect(a.translators.has(`packages/ui/src/i18n/store.ts#${seed}`), seed).toBe(true);
  });

  it("finds the functions that translate by what they call, not by a list", () => {
    // Each of these reaches the language only through others; one missing means the reader lost a
    // link (a changed import, a pattern it cannot read), and the memos that use it are unguarded.
    const must = [
      "apps/backoffice/src/labels.ts#appTitle",
      "apps/backoffice/src/labels.ts#modelTitle",
      "apps/backoffice/src/labels.ts#fieldTitle",
      "apps/backoffice/src/format.ts#optionLabel",
      "apps/backoffice/src/serverMessage.ts#serverMessage",
      "apps/backoffice/src/Chrome.tsx#buildApps",
      "apps/backoffice/src/Chrome.tsx#appBlurb",
      "apps/backoffice/src/data.tsx#recordLabelOf",
      "apps/backoffice/src/dashboards/DashboardsApp.tsx#byPosition",
      "apps/backoffice/src/dashboards/widget.ts#resolve",
      "apps/backoffice/src/serverQuery.ts#toServer",
      "apps/backoffice/src/query.ts#orderKeys",
      "apps/backoffice/src/query.ts#groupLabel",
      "packages/ui/src/i18n/labels.ts#labelFor",
      "packages/ui/src/i18n/labels.ts#inSentence",
      "packages/ui/src/i18n/store.ts#compareText",
      "packages/ui/src/i18n/store.ts#t",
      "packages/ui/src/i18n/store.ts#fmt",
    ];
    expect(must.filter((k) => !a.translators.has(k))).toEqual([]);
    // a function that is passed (`.sort(byPosition)`) is followed because it is known as a function
    expect(a.fns.has("apps/backoffice/src/dashboards/DashboardsApp.tsx#byPosition")).toBe(true);
    // members of the data context, which are built inside the provider
    for (const m of ["recordLabel", "refText"]) expect(a.members.has(m), m).toBe(true);
    // and the set is not everything: functions that only shape data are not in it
    for (const plain of ["isEmpty", "keyOf", "slug", "href", "msg", "personalLang", "setLanguage"])
      expect(names.has(plain), plain).toBe(false);
  });

  it("has no memo that translates, sorts or formats without the language in its dependencies", () => {
    const found: string[] = [];
    let seen = 0;
    for (const s of tree)
      for (const h of hooks(s.text, a.files.get(s.path)!.masked)) {
        seen++;
        const why = problem(a, s.path, h);
        if (why) found.push(`${s.path}:${h.line} ${h.hook}: ${why}`);
      }
    // The scan must have found the hooks (a broken reader would find none and pass). There are 60 at
    // the time of writing; the floor sits just under, so a reader that loses a few of them fails
    // here, and one that is right after hooks were removed lowers the number on purpose.
    expect(seen).toBeGreaterThanOrEqual(58);
    expect(
      found,
      "Add lang or locale to the dependencies, or mark the hook with i18n-memo-ok: <why>",
    ).toEqual([]);
  });

  it("explains every memo that names the language: without it the memo would be flagged", () => {
    // Take the language out of the dependencies of each hook that has it, in memory. A hook the
    // reader cannot explain this way has a dependency that nothing protects.
    const unguarded: string[] = [];
    let carrying = 0;
    for (const s of tree)
      for (const h of hooks(s.text, a.files.get(s.path)!.masked)) {
        if (!namesLanguage(h.deps) || h.exempt) continue;
        carrying++;
        const without = h
          .deps!.replace(/,?\s*(?:[\w$]+\.)?(?:lang|locale)(?![\w$])/g, "")
          .replace(/^\[\s*,/, "[");
        if (!problem(a, s.path, { ...h, deps: without })) unguarded.push(`${s.path}:${h.line}`);
      }
    // 17 hooks name the language today; a reader that stops seeing the dependencies lowers this
    expect(carrying).toBeGreaterThanOrEqual(16);
    expect(unguarded).toEqual([]);
  });
});
