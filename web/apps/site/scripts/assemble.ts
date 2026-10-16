// Builds .src (the VitePress source tree) and .generated (data for the theme and the post-build step)
// from: content/<locale>/**.md (authored), i18n/<locale>.ts (strings), packages/*.toml (apps),
// docs/MODULES.md (single source of the manifest reference) and git log (changelog).
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative } from "node:path";
import { logoSvg } from "@obop/ui/static";
import { DOCS_NAV } from "../data/docs-nav.ts";
import { DEFAULT_LOCALE, LOCALES, messages, type Locale, type Messages } from "../i18n/index.ts";
import { appBody } from "./lib/app-page.ts";
import { loadApps, type App } from "./lib/apps.ts";
import { changelogMarkdown, loadChangelog } from "./lib/changelog.ts";
import { appsIndexMirror, homeMirror, pricingMirror } from "./lib/mirror.ts";
import { GEN, GITHUB, REPO_ROOT, SITE_ROOT, SITE_URL, SRC } from "./lib/paths.ts";

export interface PageRecord {
  locale: Locale;
  /** Path without locale prefix: "/docs/agents/". */
  path: string;
  url: string;
  mdUrl: string;
  title: string;
  seoTitle: string;
  description: string;
  section: string;
  fallback: boolean;
  alternates: { lang: string; url: string }[];
  headings: string[];
  text: string;
}

interface Raw {
  fm: Record<string, string>;
  body: string;
  file: string;
}

const prefixOf = (l: Locale) => (l === DEFAULT_LOCALE ? "" : `/${l}`);

function parseFm(src: string, file: string): Raw {
  const m = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(src);
  if (!m) throw new Error(`${file}: missing front matter`);
  const fm: Record<string, string> = {};
  for (const line of m[1]!.split("\n")) {
    const kv = /^([A-Za-z][\w-]*):\s*(.*)$/.exec(line);
    if (!kv) continue;
    let v = kv[2]!.trim();
    if (/^".*"$/.test(v)) v = JSON.parse(v) as string;
    else if (/^'.*'$/.test(v)) v = v.slice(1, -1);
    fm[kv[1]!] = v;
  }
  if (!fm["title"] || !fm["description"])
    throw new Error(`${file}: title and description are required`);
  return { fm, body: m[2]!.replace(/^\n+/, ""), file };
}

function walk(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? walk(p) : p.endsWith(".md") ? [p] : [];
  });
}

/** content/en/docs/getting-started.md -> /docs/getting-started/ ; .../index.md -> directory. */
function pathOf(rel: string): string {
  const p = rel.replace(/\.md$/, "").replace(/(^|\/)index$/, "");
  return p ? `/${p}/` : "/";
}

function yaml(fm: Record<string, unknown>): string {
  return `---\n${Object.entries(fm)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
    .join("\n")}\n---\n\n`;
}

function plain(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, (b) => b.replace(/```\w*/g, ""))
    .replace(/<[^>]+>/g, " ")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/[*_`|>]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function headingsOf(md: string): string[] {
  return [...md.matchAll(/^#{2,3}\s+(.+)$/gm)].map((h) => h[1]!.replace(/[`*]/g, ""));
}

function ghLink(href: string): string {
  const clean = href.replace(/^(\.\.\/)+/, "").replace(/^\.\//, "");
  return `${GITHUB}/blob/main/docs/${clean}`;
}

/** The manifest reference is docs/MODULES.md, included at build time (single source). */
function moduleGuide(): string {
  const src = readFileSync(join(REPO_ROOT, "docs/MODULES.md"), "utf8");
  return src
    .replace(/^# .*\n+/, "")
    .replace(
      /\]\((?!https?:|#|\/)([^)]+?\.md[^)]*)\)/g,
      (_m, href: string) => `](${ghLink(href)})`,
    );
}

const TOKENS: Record<string, (apps: App[]) => string> = {
  catalog: (apps) => {
    const rows = apps
      .filter((a) => !a.builtin)
      .map(
        (a) =>
          `| [${a.name}](/apps/${a.slug}/) | \`${a.id}\` | ${a.version} | ${a.description.replace(/\|/g, "\\|")} |`,
      );
    return ["| App | Package id | Version | Description |", "|---|---|---|---|", ...rows].join(
      "\n",
    );
  },
  changelog: () => {
    const { entries } = loadChangelog();
    return changelogMarkdown(entries);
  },
  moduleguide: () => moduleGuide(),
};

function expandTokens(body: string, apps: App[]): string {
  return body.replace(/^%%(\w+)%%$/gm, (_m, name: string) => {
    const fn = TOKENS[name];
    if (!fn) throw new Error(`unknown token %%${name}%%`);
    return fn(apps);
  });
}

const NON_PAGE = /^\/(app|community|status|openapi)(\/|$)|\.[a-z0-9]+$|^\/\.well-known/;

/** Prefix site-absolute links with the locale when the target is a page of this site. */
function localize(md: string, prefix: string, pagePaths: Set<string>): string {
  if (!prefix) return md;
  const fix = (p: string) => {
    const bare = p.split(/[#?]/)[0]!;
    if (NON_PAGE.test(bare) || !pagePaths.has(bare.endsWith("/") ? bare : `${bare}/`)) return p;
    return `${prefix}${p}`;
  };
  return md
    .replace(/\]\((\/[^)\s]*)\)/g, (_m, p: string) => `](${fix(p)})`)
    .replace(/href="(\/[^"]*)"/g, (_m, p: string) => `href="${fix(p)}"`);
}

function rm(p: string) {
  rmSync(p, { recursive: true, force: true });
}
function write(file: string, content: string) {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, content);
}

export function assemble(): PageRecord[] {
  rm(SRC);
  rm(GEN);
  mkdirSync(SRC, { recursive: true });
  mkdirSync(GEN, { recursive: true });
  const apps = loadApps();

  // 1. Authored pages per locale.
  const authored = new Map<Locale, Map<string, Raw>>();
  for (const l of LOCALES) {
    const dir = join(SITE_ROOT, "content", l);
    const map = new Map<string, Raw>();
    for (const f of walk(dir)) {
      const rel = relative(dir, f).split("\\").join("/");
      map.set(pathOf(rel), {
        ...parseFm(readFileSync(f, "utf8"), f),
        file: `web/apps/site/content/${l}/${rel}`,
      });
    }
    authored.set(l, map);
  }
  const en = authored.get("en")!;

  const generatedPaths = [
    "/",
    "/apps/",
    "/pricing/",
    "/search/",
    ...apps.map((a) => `/apps/${a.slug}/`),
  ];
  const pagePaths = new Set<string>([...en.keys(), ...generatedPaths]);
  for (const g of DOCS_NAV)
    for (const p of g.pages) if (!en.has(p)) throw new Error(`docs nav page missing: ${p}`);
  for (const l of LOCALES) {
    for (const p of authored.get(l)!.keys())
      if (!en.has(p)) throw new Error(`${l}: ${p} has no English source`);
  }

  const records: PageRecord[] = [];
  const out: { rec: PageRecord; fm: Record<string, unknown>; body: string; mirror: string }[] = [];

  const add = (
    locale: Locale,
    path: string,
    p: {
      title: string;
      seoTitle?: string;
      description: string;
      layout: string;
      section: string;
      body: string;
      mirror?: string;
      fallback?: boolean;
      extra?: Record<string, unknown>;
      editPath: string;
    },
  ) => {
    const prefix = prefixOf(locale);
    const url = `${prefix}${path}`;
    const mirrorBody = p.mirror ?? p.body;
    const rec: PageRecord = {
      locale,
      path,
      url,
      mdUrl: path === "/" ? `${prefix}/index.md` : `${prefix}${path.slice(0, -1)}.md`,
      title: p.title,
      seoTitle: p.seoTitle ?? p.title,
      description: p.description,
      section: p.section,
      fallback: p.fallback ?? false,
      alternates: [],
      headings: headingsOf(mirrorBody),
      text: plain(mirrorBody).slice(0, 4000),
    };
    records.push(rec);
    out.push({
      rec,
      fm: {
        title: p.title,
        seoTitle: p.seoTitle,
        description: p.description,
        layout: p.layout,
        locale,
        pagePath: path,
        section: p.section,
        fallback: p.fallback ?? false,
        editPath: p.editPath,
        ...p.extra,
      },
      body: p.body,
      mirror: mirrorBody,
    });
  };

  for (const locale of LOCALES) {
    const m: Messages = messages[locale];
    const prefix = prefixOf(locale);
    const loc = (md: string) => localize(md, prefix, pagePaths);

    // 2. Generated pages (always translated: strings come from i18n/<locale>.ts).
    add(locale, "/", {
      title: m.tagline,
      seoTitle: `obop: ${m.tagline}`,
      description: m.description,
      layout: "home",
      section: "home",
      body: "",
      mirror: homeMirror(m, prefix, apps),
      editPath: `web/apps/site/i18n/${locale}.ts`,
    });
    add(locale, "/apps/", {
      title: m.apps.title,
      description: m.apps.description,
      layout: "apps-index",
      section: "apps",
      body: "",
      mirror: appsIndexMirror(m, prefix, apps),
      editPath: `web/apps/site/i18n/${locale}.ts`,
    });
    add(locale, "/pricing/", {
      title: m.pricing.title,
      description: m.pricing.description,
      layout: "pricing",
      section: "pricing",
      body: "",
      mirror: pricingMirror(m),
      editPath: `web/apps/site/i18n/${locale}.ts`,
    });
    add(locale, "/search/", {
      title: m.search.title,
      description: m.search.description,
      layout: "search",
      section: "search",
      body: "",
      mirror: m.search.description,
      editPath: `web/apps/site/i18n/${locale}.ts`,
    });
    for (const app of apps) {
      const tagline = m.apps.taglines[app.id] ?? app.tagline;
      add(locale, `/apps/${app.slug}/`, {
        title: app.name,
        seoTitle: `${app.name}: ${tagline}`,
        description:
          locale === "en"
            ? `${tagline}. ${app.purpose.split(". ")[0]!.replace(/\.$/, "")}.`
            : `${tagline}. ${m.apps.lead.split(". ")[0]!.replace(/\.$/, "")}.`,
        layout: "app",
        section: "apps",
        body: appBody(app, locale, m, false, prefix),
        mirror: appBody(app, locale, m, true, prefix),
        editPath: app.builtin ? `web/apps/site/data/builtin/${app.id}.toml` : app.source,
        extra: {
          appId: app.id,
          appSlug: app.slug,
          category: app.category,
          builtin: app.builtin,
          version: app.version,
          tagline,
          lead: tagline,
        },
      });
    }

    // 3. Authored pages: translated when present, otherwise the English text as a fallback.
    for (const [path, en0] of en) {
      const own = authored.get(locale)!.get(path);
      const raw = own ?? en0;
      const fallback = !own;
      const body = loc(expandTokens(raw.body, apps));
      const isDoc = path.startsWith("/docs/");
      const guide = path === "/docs/reference/module-manifest/";
      add(locale, path, {
        title: raw.fm["title"]!,
        seoTitle: raw.fm["seoTitle"],
        description: raw.fm["description"]!,
        layout: raw.fm["layout"] ?? (isDoc ? "doc" : "page"),
        section: raw.fm["section"] ?? (isDoc ? "docs" : "page"),
        body,
        fallback,
        editPath: guide ? "docs/MODULES.md" : raw.file,
        extra: { lead: raw.fm["lead"], eyebrow: raw.fm["eyebrow"] },
      });
    }
  }

  // 4. Alternates: translated (non-fallback) variants only, plus x-default for English.
  const byPath = new Map<string, PageRecord[]>();
  for (const r of records) byPath.set(r.path, [...(byPath.get(r.path) ?? []), r]);
  for (const r of records) {
    const group = (byPath.get(r.path) ?? []).filter((x) => !x.fallback);
    r.alternates = r.fallback
      ? []
      : group.map((x) => ({ lang: messages[x.locale].htmlLang, url: x.url }));
  }

  // 5. Docs navigation per locale.
  const docsNav: Record<string, unknown> = {};
  for (const l of LOCALES) {
    docsNav[l] = DOCS_NAV.map((g) => ({
      group: g.group,
      title: messages[l].docs.groups[g.group],
      pages: g.pages.map((p) => {
        const r = records.find((x) => x.locale === l && x.path === p)!;
        return { path: p, url: r.url, title: r.title, fallback: r.fallback };
      }),
    }));
  }

  // 6. Write the VitePress source tree and the Markdown mirrors.
  for (const { rec, fm, body, mirror } of out) {
    const file = rec.path === "/" ? "index.md" : `${rec.path.slice(1)}index.md`;
    const dest = join(SRC, rec.locale === DEFAULT_LOCALE ? "" : rec.locale, file);
    const frontmatter = { ...fm, alternates: rec.alternates, mdUrl: rec.mdUrl };
    write(dest, `${yaml(frontmatter)}${body}\n`);
    const canonicalUrl = `${SITE_URL}${rec.fallback ? rec.path : rec.url}`;
    const mirrorFile = join(GEN, "mirror", rec.mdUrl);
    write(
      mirrorFile,
      `---\ntitle: ${JSON.stringify(rec.title)}\ndescription: ${JSON.stringify(rec.description)}\ncanonical: ${JSON.stringify(canonicalUrl)}\nlang: ${JSON.stringify(messages[rec.locale].htmlLang)}\n---\n\n# ${rec.title}\n\n${mirror.trim()}\n`,
    );
  }

  // The 404 page: VitePress renders its own 404.html as an empty shell, so the real one is a normal
  // page that postbuild copies to 404.html.
  write(
    join(SRC, "page-not-found/index.md"),
    `---\ntitle: "Page not found"\ndescription: "This page does not exist."\nlayout: "notfound"\nlocale: "en"\nnoindex: true\n---\n`,
  );

  // 7. Static assets: logos, ASCII field, client scripts.
  const brandDir = join(SRC, "public/brand");
  const svgDoc = (inner: string, color: string) =>
    inner.replace("<svg ", `<svg xmlns="http://www.w3.org/2000/svg" style="color:${color}" `);
  write(join(brandDir, "obop-logo.svg"), svgDoc(logoSvg("full", 48, "obop"), "#1c1b22"));
  write(join(brandDir, "obop-logo-white.svg"), svgDoc(logoSvg("full", 48, "obop"), "#f2f1f7"));
  write(join(brandDir, "obop-logo-black.svg"), svgDoc(logoSvg("full", 48, "obop"), "#000000"));
  write(join(brandDir, "obop-mark.svg"), svgDoc(logoSvg("mark", 48, "obop"), "#1c1b22"));
  write(join(brandDir, "obop-wordmark.svg"), svgDoc(logoSvg("word", 48, "obop"), "#1c1b22"));
  cpSync(join(SITE_ROOT, "public"), join(SRC, "public"), { recursive: true });
  mkdirSync(join(SRC, "public/site"), { recursive: true });
  cpSync(join(REPO_ROOT, "design/ascii-field.js"), join(SRC, "public/site/ascii-field.js"));
  cpSync(join(SITE_ROOT, "client"), join(SRC, "public/site"), { recursive: true });

  // 8. Data for the theme and the post-build step.
  write(
    join(GEN, "apps.json"),
    JSON.stringify(
      apps.map((a) => ({
        id: a.id,
        slug: a.slug,
        name: a.name,
        category: a.category,
        builtin: a.builtin,
        tagline: a.tagline,
      })),
    ),
  );
  write(join(GEN, "docs-nav.json"), JSON.stringify(docsNav));
  write(join(GEN, "pages.json"), JSON.stringify(records));
  const titles: Record<string, Record<string, string>> = {};
  for (const r of records) (titles[r.locale] ??= {})[r.path] = r.title;
  write(join(GEN, "titles.json"), JSON.stringify(titles));
  write(join(GEN, "apps-full.json"), JSON.stringify(apps));

  // 9. Translation status must cover every translated page.
  for (const l of LOCALES) {
    if (l === DEFAULT_LOCALE) continue;
    const file = join(SITE_ROOT, "content", l, "translation-status.json");
    if (!existsSync(file)) throw new Error(`missing ${relative(REPO_ROOT, file)}`);
    const status = JSON.parse(readFileSync(file, "utf8")) as Record<string, string>;
    const expected = [
      "ui",
      "home",
      "apps",
      "pricing",
      ...[...authored.get(l)!.keys()].map((p) => p),
    ];
    for (const k of expected)
      if (!(k in status)) throw new Error(`${l}: translation-status.json lacks ${k}`);
    for (const [k, v] of Object.entries(status)) {
      if (!["machine-drafted", "reviewed"].includes(v))
        throw new Error(`${l}: bad status for ${k}`);
      if (!expected.includes(k))
        throw new Error(`${l}: translation-status.json lists unknown page ${k}`);
    }
  }
  return records;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const records = assemble();
  console.log(`assembled ${records.length} pages for ${LOCALES.length} locales`);
}
