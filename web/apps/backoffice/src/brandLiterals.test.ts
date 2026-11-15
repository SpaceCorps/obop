import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vite-plus/test";
import { catalogueHits, logoImports, sourceHits, type Hit } from "./brandLiterals.ts";

// docs/spec/branding.md, section 13 ("TypeScript, in S2"). The non-test `.ts` and `.tsx` of the back
// office and of `@obop/ui` (the Brand book in `brand/` and the Guide document obop itself and are left
// out), and the values of the back office catalogues, may say `obop` only where a row of ALLOWED says
// why. A new literal is a deliberate act: it needs a row here, with the reason, in the same change.
// `Logo`, obop's own mark, may be imported only by `InstanceLogo` and by the Brand book.
//
// What the scan does not see: text built at run time, text in a dependency, a file that is not on
// the list, a name inside a longer literal that is on the list for another reason, text in a binary
// asset, and a `Logo` reached through a namespace import or through `export *` (the package index of
// `@obop/ui` is the one `export *` of the mark, by design). A named re-export is seen. It does not
// stop a determined workaround.

const here = new URL(".", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const WEB = join(here, "../../..");
const ROOTS = ["apps/backoffice/src", "packages/ui/src"];
const BRAND_DIR = "apps/backoffice/src/brand";
const rel = (p: string) => relative(WEB, p).replaceAll("\\", "/");

function walk(dir: string, withBrand: boolean, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "node_modules" && (withBrand || rel(p) !== BRAND_DIR)) walk(p, withBrand, out);
    } else if (
      /\.tsx?$/.test(name) &&
      !/\.(test|d)\.tsx?$/.test(name) &&
      // The Guide documents obop itself, and the readers of the source scans are test support.
      (withBrand || name !== "Guide.tsx") &&
      !["brandLiterals.ts", "i18nGuard.ts"].includes(name)
    )
      out.push(p);
  }
  return out;
}

/**
 * A row: where the word is allowed, the literal (or, for a catalogue, the English message), and why.
 * `file` is a path relative to `web/`, or `*` for a message that is the same wherever it is used
 * and in the catalogues.
 */
type Allowed = { file: string; text: string; why: string };

const BO = "apps/backoffice/src";
const UI = "packages/ui/src";

const STORAGE = "A key of the browser's storage (or a cookie), read back by the code; never shown.";
const CONSOLE = "A line for the browser console of whoever debugs it; never on a screen.";
const WIRE = "The name of a tag or a header that the server and the page agree on; never shown.";
const OWN_MARK =
  "The accessible name of the product's own mark, drawn only where the page has no brand (InstanceLogo falls back to it).";
const ADMIN =
  "Wording for administrators on a screen only they open, which names the software they run on purpose (spec section 11).";

const ALLOWED: Allowed[] = [
  // Browser storage keys, cookies and console prefixes.
  { file: `${BO}/api.ts`, text: "obop-sso-place", why: STORAGE },
  { file: `${BO}/App.tsx`, text: "obop-recent", why: STORAGE },
  { file: `${BO}/App.tsx`, text: "obop-token", why: STORAGE },
  { file: `${BO}/App.tsx`, text: "obop-user", why: STORAGE },
  { file: `${BO}/favorites.ts`, text: "obop-fav:", why: STORAGE },
  { file: `${BO}/ModelPage.tsx`, text: "obop-view-", why: STORAGE },
  { file: `${BO}/ModelPage.tsx`, text: "obop-pagesize", why: STORAGE },
  { file: `${BO}/theme.ts`, text: "obop-theme", why: STORAGE },
  { file: `${BO}/views/KanbanView.tsx`, text: "obop-fold:", why: STORAGE },
  { file: `${BO}/views/ListView.tsx`, text: "obop-cols:", why: STORAGE },
  { file: `${UI}/i18n/store.ts`, text: "obop-lang", why: STORAGE },
  { file: `${UI}/i18n/store.ts`, text: "obop-region", why: STORAGE },
  {
    file: `${UI}/i18n/store.ts`,
    text: "obop_lang",
    why: "The cookie the server reads the language from (wire name); never shown.",
  },
  { file: `${BO}/ErrorBoundary.tsx`, text: "[obop] a screen failed to render", why: CONSOLE },
  { file: `${BO}/main.tsx`, text: "[obop] unhandled promise rejection", why: CONSOLE },
  // Names the server and the page agree on, and the product's own mark.
  { file: `${UI}/i18n/store.ts`, text: "obop-default-language", why: WIRE },
  { file: `${UI}/brand.ts`, text: "obop-brand", why: WIRE },
  {
    file: `${UI}/brand.ts`,
    text: "obop",
    why: "The stem of the recovery-codes file where the page has no brand: the control-plane app keeps obop-recovery-codes.txt (spec section 6).",
  },
  {
    file: `${BO}/Chrome.tsx`,
    text: '{pageBrand() ? null : <span className="sr-only">obop</span>}',
    why: "The text a link to the home screen is named by where the page has no brand; with one, the logo shows the name.",
  },
  { file: `${UI}/instance-logo.tsx`, text: "obop", why: OWN_MARK },
  { file: `${UI}/logo.tsx`, text: "obop", why: OWN_MARK },
  { file: `${UI}/static.ts`, text: "obop", why: OWN_MARK },
  // Administrators' screens: the module builder and upload, the update page, the connectors.
  {
    file: `${BO}/ModuleBuilder.tsx`,
    text: "<h3>obop.toml</h3>",
    why: "The name of the module file format, in the module builder (administrators).",
  },
  {
    file: `${BO}/ModuleBuilder.tsx`,
    text: 'author = "Built in obop"',
    why: "The author line of a module an administrator builds in the module builder.",
  },
  {
    file: `${BO}/updates/UpdatesPage.tsx`,
    text: "obop update check",
    why: "A command line an operator types; it is the name of the program (administrators).",
  },
  {
    file: "*",
    text: "A reference or a place, for example “DPA signed 2026-10-01, folder Legal”. obop does not check the agreement.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "An obop.toml file: models, fields with data classes, permission groups and actions. It is checked here before anything is installed.",
    why: ADMIN,
  },
  { file: "*", text: "It needs {modules}, so the ranges are written into obop.toml.", why: ADMIN },
  { file: "*", text: "Text of the obop.toml", why: ADMIN },
  { file: "*", text: "Upload an obop.toml file", why: ADMIN },
  {
    file: "*",
    text: "obop ships no client id or secret. Register an application with the provider and enter its credentials here.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "OBOP_BACKUP_DIR is not set, so a backup cannot be taken here. You can still update if you have your own backup (obop backup, or pg_dump).",
    why: ADMIN,
  },
  { file: "*", text: "1. Register obop with Google", why: ADMIN },
  {
    file: "*",
    text: "A Drive is configured with environment variables, which take precedence over this page. Remove OBOP_GDRIVE_BROWSE_REFRESH_TOKEN to use the settings here.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "Disconnect Google Drive? obop forgets the sign-in and nobody can browse the Drive until you connect again. To withdraw Google’s side too, remove obop at myaccount.google.com/permissions.",
    why: ADMIN,
  },
  { file: "*", text: "Disconnected. The sign-in was removed from obop.", why: ADMIN },
  {
    file: "*",
    text: "The server does not know its own address, so it cannot start a sign-in. Set OBOP_PUBLIC_URL (for example https://obop.example.com) and restart; the address to register is then that address followed by /oauth/callback.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "Browse your Google Drive from the Documents app, add a file, such as a contract written as a Google Doc, as a document, and make folders, Google Docs and uploads in your workspace drive. obop never changes or deletes what is already in Drive.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "obop works in this one drive. Browsing, search and imports stay inside it, and new folders, documents and uploads are made there.",
    why: ADMIN,
  },
  {
    file: "*",
    text: "The last sign-in only let obop read. Press “Sign in again” and allow creating files.",
    why: ADMIN,
  },
];

const key = (h: Pick<Hit, "file" | "text">) => `${h.file}\u0000${h.text}`;
const allows = (row: Allowed, h: Hit) =>
  row.text === h.text && (row.file === h.file || (row.file === "*" && h.file !== ""));

/** Every hit in the sources and the catalogues of the working tree. */
function found(): Hit[] {
  const hits: Hit[] = [];
  for (const root of ROOTS)
    for (const p of walk(join(WEB, root), false))
      hits.push(...sourceHits(rel(p), readFileSync(p, "utf8")));
  const catalogues = join(WEB, "apps/backoffice/locales");
  for (const lang of readdirSync(catalogues))
    hits.push(
      ...catalogueHits(
        `apps/backoffice/locales/${lang}/messages.json`,
        JSON.parse(readFileSync(join(catalogues, lang, "messages.json"), "utf8")),
      ),
    );
  return hits;
}

describe("the word obop in the back office", () => {
  const hits = found();

  it("is in no literal, text or catalogue value that is not a row of the table", () => {
    const loose = new Map<string, Hit>();
    for (const h of hits) if (!ALLOWED.some((row) => allows(row, h))) loose.set(key(h), h);
    expect(
      [...loose.values()].map(
        (h) =>
          `${h.file}${h.line ? `:${h.line}` : ""} (${h.kind}) ${JSON.stringify(h.text)}: say it without the product name, or add a row to ALLOWED with the reason`,
      ),
    ).toEqual([]);
  });

  it("has no row that no literal needs, so that a removed literal takes its row away", () => {
    const stale = ALLOWED.filter((row) => !hits.some((h) => allows(row, h)));
    expect(stale.map((r) => `${r.file}: ${JSON.stringify(r.text)}`)).toEqual([]);
  });

  it("gives a reason for every row", () => {
    for (const row of ALLOWED) expect(row.why.trim().length, row.text).toBeGreaterThan(20);
  });

  it("has no row twice", () => {
    const keys = ALLOWED.map(key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("reads the catalogues of all seven languages and the sources of both packages", () => {
    expect(readdirSync(join(WEB, "apps/backoffice/locales")).sort()).toEqual([
      "de",
      "en",
      "es",
      "fi",
      "fr",
      "pl",
      "sv",
    ]);
    const files = ROOTS.flatMap((r) => walk(join(WEB, r), false).map(rel));
    expect(files).toContain(`${BO}/SignIn.tsx`);
    expect(files).toContain(`${UI}/instance-logo.tsx`);
    expect(files.some((f) => f.startsWith(`${BRAND_DIR}/`))).toBe(false);
    expect(files.some((f) => f.endsWith("Guide.tsx"))).toBe(false);
    expect(hits.some((h) => h.kind === "catalogue")).toBe(true);
  });
});

describe("Logo, obop's own mark", () => {
  it("is imported only by InstanceLogo and by the Brand book", () => {
    const importers = ROOTS.flatMap((r) => walk(join(WEB, r), true))
      .filter((p) => logoImports(readFileSync(p, "utf8")))
      .map(rel)
      .sort();
    // The back office's own barrel re-exports the primitives of `@obop/ui` by name, `Logo` among
    // them. A file that imports the mark from the barrel is found as an importer itself, so the
    // barrel is the one re-export allowed; a second file that re-exports `Logo` is a failure.
    const BARREL = "apps/backoffice/src/ui.tsx";
    const outsiders = importers.filter(
      (f) => f !== `${UI}/instance-logo.tsx` && f !== BARREL && !f.startsWith(`${BRAND_DIR}/`),
    );
    expect(outsiders, "draw the logo of the instance with InstanceLogo").toEqual([]);
    // And the two that may, do.
    expect(importers).toContain(`${UI}/instance-logo.tsx`);
    expect(importers.some((f) => f.startsWith(`${BRAND_DIR}/`))).toBe(true);
  });
});

describe("the reader", () => {
  const hit = (src: string) => sourceHits("x.tsx", src).map((h) => `${h.kind}:${h.text}`);

  it("sees a string, a template text, an attribute and JSX text", () => {
    expect(hit(`const a = "the obop mark";`)).toEqual(["string:the obop mark"]);
    expect(hit("const a = `welcome to obop ${name}`;")).toEqual(["template:welcome to obop "]);
    expect(hit("const a = `${name} on obop`;")).toEqual(["template: on obop"]);
    expect(hit(`const a = <img alt="obop" />;`)).toEqual(["string:obop"]);
    expect(hit("const a = <p>Powered by obop</p>;")).toEqual([
      "code:const a = <p>Powered by obop</p>;",
    ]);
    expect(hit("x(`a${y}b${'obop'}`);")).toEqual(["string:obop"]);
  });

  it("sees the word in any case", () => {
    expect(hit(`const a = "OBOP_URL and Obop";`)).toEqual(["string:OBOP_URL and Obop"]);
  });

  it("does not see a comment, a regular expression, an import path or a longer name", () => {
    expect(hit(`// the obop mark\nconst a = 1; /* obop */`)).toEqual([]);
    expect(hit(`const r = /obop/;`)).toEqual([]);
    expect(hit(`import { x } from "@obop/ui";\nexport * from "@obop/ui/styles.css";`)).toEqual([]);
    expect(hit(`const m = await import("@obop/ui");`)).toEqual([]);
    expect(hit(`const obopRecords = 1; const OBOP_X = 2; obop_y();`)).toEqual([]);
  });

  it("sees an identifier that is the word on its own", () => {
    expect(hit(`const obop = 1;`)).toEqual(["code:const obop = 1;"]);
  });

  it("reads a catalogue by the English id and by the text", () => {
    expect(
      catalogueHits("c.json", {
        "About obop": "Über obop",
        Plain: "Schlicht",
        Other: "obop-x",
      }).map((h) => h.text),
    ).toEqual(["About obop", "Other"]);
  });

  it("finds a named import of Logo and nothing else", () => {
    expect(logoImports(`import { Icon, Logo } from "./ui.tsx";`)).toBe(true);
    expect(logoImports(`import {\n  Button,\n  Logo,\n} from "@obop/ui";`)).toBe(true);
    expect(logoImports(`import { Logo as Mark } from "@obop/ui";`)).toBe(true);
    expect(logoImports(`import { type Logo } from "@obop/ui";`)).toBe(true);
    expect(logoImports(`import { InstanceLogo, LogoProps } from "@obop/ui";`)).toBe(false);
    expect(logoImports(`import { Icon } from "./ui.tsx"; // Logo is not imported`)).toBe(false);
    // A re-export by name makes the mark importable from a file that is not on the list.
    expect(logoImports(`export { Logo } from "@obop/ui";`)).toBe(true);
    expect(logoImports(`export { Icon, Logo as Mark } from "./logo.tsx";`)).toBe(true);
    expect(logoImports(`export type { Logo } from "./logo.tsx";`)).toBe(true);
    // The package index re-exports the file as a whole and names nothing.
    expect(logoImports(`export * from "./logo.tsx";`)).toBe(false);
    expect(logoImports(`export { InstanceLogo } from "./instance-logo.tsx";`)).toBe(false);
    expect(logoImports(`export const Logo = 1; // not from a module`)).toBe(false);
  });
});
