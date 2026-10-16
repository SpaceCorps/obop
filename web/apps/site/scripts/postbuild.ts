// Everything that needs the finished pages: Markdown mirrors, search indexes, social cards,
// sitemaps, robots.txt, llms.txt, security.txt. Runs after `vitepress build`.
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { DOCS_NAV } from "../data/docs-nav.ts";
import { LOCALES, messages } from "../i18n/index.ts";
import type { PageRecord } from "./assemble.ts";
import { loadChangelog } from "./lib/changelog.ts";
import { appleTouchIcon, cardSvg, renderPng } from "./lib/og.ts";
import { DIST, GEN, GITHUB, SITE_URL, SRC } from "./lib/paths.ts";

function write(rel: string, content: string | Buffer) {
  const file = join(DIST, rel);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, content);
}
const xml = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Social card path for a page, shared with the <meta> tags written in .vitepress/config.ts. */
export const ogPath = (locale: string, path: string) =>
  `/og/${locale}${path === "/" ? "/home" : path.slice(0, -1)}.png`;

/** AI crawlers and agents, allowed by name so the intent is explicit. */
const AI_AGENTS = [
  "GPTBot",
  "ChatGPT-User",
  "OAI-SearchBot",
  "ClaudeBot",
  "Claude-User",
  "Claude-SearchBot",
  "anthropic-ai",
  "Claude-Web",
  "PerplexityBot",
  "Perplexity-User",
  "Google-Extended",
  "GoogleOther",
  "Applebot-Extended",
  "Bingbot",
  "Amazonbot",
  "Meta-ExternalAgent",
  "Meta-ExternalFetcher",
  "CCBot",
  "cohere-ai",
  "DuckAssistBot",
  "MistralAI-User",
  "YouBot",
];

export async function postbuild(records: PageRecord[]): Promise<void> {
  const real = records.filter((r) => !r.fallback);
  const { entries } = loadChangelog();
  const lastmod = entries[0]?.date ?? new Date().toISOString().slice(0, 10);

  // 1. No framework JavaScript is shipped (see transformHtml in .vitepress/config.ts).
  rmSync(join(DIST, "hashmap.json"), { force: true });
  const assets = join(DIST, "assets");
  for (const f of readdirSync(assets)) if (f.endsWith(".js")) rmSync(join(assets, f));
  for (const r of records) {
    const file = join(DIST, r.url === "/" ? "index.html" : `${r.url.slice(1)}index.html`);
    let html = readFileSync(file, "utf8");
    html = html.replace(/<script id="check-mac-os">[\s\S]*?<\/script>\s*/g, "").replace(/\n\s*\n(\s*\n)+/g, "\n");
    writeFileSync(file, html);
  }

  // Static files (logos, client scripts): copied here so nothing depends on how the bundler treats public/.
  cpSync(join(SRC, "public"), DIST, { recursive: true });

  // 2. Markdown mirrors (made by assemble.ts) next to every page.
  cpSync(join(GEN, "mirror"), DIST, { recursive: true });

  // 3. Search indexes, one per language: [url, title, text, headings].
  for (const l of LOCALES) {
    const idx = records
      .filter((r) => r.locale === l && r.section !== "search")
      .map((r) => [r.url, r.title, r.text.slice(0, 2500), r.headings.join(" ")]);
    write(`search/${l}.json`, JSON.stringify(idx));
  }

  // 4. Social cards for every translated page, and the touch icon.
  const seen = new Map<string, Buffer>();
  const labelFor = (locale: (typeof LOCALES)[number], section: string) =>
    messages[locale].nav.find((n) => n.href === `/${section}/`)?.label ?? "obop";
  for (const r of real) {
    const card = { title: r.title, description: r.description, label: labelFor(r.locale, r.section) };
    const key = createHash("sha1").update(JSON.stringify(card)).digest("hex");
    let png = seen.get(key);
    if (!png) {
      png = renderPng(cardSvg(card));
      seen.set(key, png);
    }
    write(ogPath(r.locale, r.path), png);
  }
  write("og/default.png", renderPng(cardSvg({ title: "One business. One platform.", description: messages.en.description, label: "obop" })));
  write("apple-touch-icon.png", appleTouchIcon());

  // 5. Sitemaps: one per language with hreflang alternates, and an index that includes the forum's.
  const loc = (u: string) => `${SITE_URL}${u}`;
  for (const l of LOCALES) {
    const urls = real
      .filter((r) => r.locale === l)
      .map((r) => {
        const alts = r.alternates.map((a) => `    <xhtml:link rel="alternate" hreflang="${a.lang}" href="${xml(loc(a.url))}"/>`);
        if (r.alternates.length) alts.push(`    <xhtml:link rel="alternate" hreflang="x-default" href="${xml(loc(r.path))}"/>`);
        return `  <url>\n    <loc>${xml(loc(r.url))}</loc>\n    <lastmod>${lastmod}</lastmod>\n${alts.join("\n")}\n  </url>`;
      });
    write(
      `sitemap-${l}.xml`,
      `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n${urls.join("\n")}\n</urlset>\n`,
    );
  }
  const index = [...LOCALES.map((l) => `sitemap-${l}.xml`), "community/sitemap.xml"];
  write(
    "sitemap.xml",
    `<?xml version="1.0" encoding="UTF-8"?>\n<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${index
      .map((f) => `  <sitemap>\n    <loc>${loc(`/${f}`)}</loc>\n    <lastmod>${lastmod}</lastmod>\n  </sitemap>`)
      .join("\n")}\n</sitemapindex>\n`,
  );

  // 6. robots.txt: everything is allowed, AI crawlers by name.
  write(
    "robots.txt",
    [
      "# obop: everything public is open to people, search engines and AI agents.",
      "User-agent: *",
      "Allow: /",
      "",
      ...AI_AGENTS.flatMap((a) => [`User-agent: ${a}`, "Allow: /", ""]),
      `Sitemap: ${loc("/sitemap.xml")}`,
      `Sitemap: ${loc("/community/sitemap.xml")}`,
      "",
    ].join("\n"),
  );

  // 7. llms.txt and llms-full.txt (llmstxt.org).
  const en = real.filter((r) => r.locale === "en");
  const by = (p: string) => en.find((r) => r.path === p);
  const link = (r: PageRecord) => `- [${r.title}](${loc(r.mdUrl)}): ${r.description}`;
  const section = (title: string, paths: string[]) => [
    `## ${title}`,
    "",
    ...paths.map(by).filter((r): r is PageRecord => !!r).map(link),
    "",
  ];
  const groupTitle: Record<string, string> = {
    start: "Documentation",
    concepts: "Concepts",
    agents: "Agents: set up AI agents to work with obop",
    reference: "Reference",
  };
  const llms = [
    "# obop",
    "",
    `> ${messages.en.description} Open source, written in Rust, runs on SQLite or PostgreSQL. Status: beta; see the roadmap for what is built.`,
    "",
    "Each link below is the Markdown version of a page. Every page of the site also exists as Markdown at its address with .md instead of the trailing slash. Prices on the site are planned and not charged yet.",
    "",
    ...DOCS_NAV.flatMap((g) => section(groupTitle[g.group]!, g.pages)),
    ...section("Product", ["/platform/", "/marketplace/", "/pricing/", "/apps/", "/roadmap/", "/changelog/"]),
    "## Apps",
    "",
    ...en.filter((r) => /^\/apps\/[^/]+\/$/.test(r.path)).map(link),
    "",
    ...section("Company and trust", ["/about/", "/security/", "/privacy/", "/terms/", "/contact/", "/brand/", "/about/community/", "/about/status/"]),
    "## Optional",
    "",
    ...LOCALES.filter((l) => l !== "en").map((l) => `- [${messages[l].langName}](${loc(`/${l}/index.md`)}): the site in ${messages[l].langName}`),
    `- [Source code](${GITHUB}): the repository`,
    `- [Full documentation in one file](${loc("/llms-full.txt")}): the text of every English page`,
    "",
  ];
  write("llms.txt", llms.join("\n"));
  const full = en
    .filter((r) => r.section !== "search")
    .map((r) => {
      const md = readFileSync(join(GEN, "mirror", r.mdUrl), "utf8").replace(/^---[\s\S]*?---\n+/, "");
      return `${md.trim()}\n\nSource: ${loc(r.url)}`;
    });
  write("llms-full.txt", `# obop: full text\n\n> ${messages.en.description}\n\n${full.join("\n\n---\n\n")}\n`);

  // 8. Trust files.
  const expires = new Date(Date.now() + 365 * 24 * 3600 * 1000).toISOString().replace(/\.\d+Z$/, "Z");
  write(
    ".well-known/security.txt",
    [
      `Contact: ${GITHUB}/security/advisories/new`,
      `Expires: ${expires}`,
      "Preferred-Languages: en, de, fr, pl, fi, sv, es",
      `Canonical: ${loc("/.well-known/security.txt")}`,
      `Policy: ${loc("/security/")}`,
      "",
    ].join("\n"),
  );
  write("humans.txt", "/* obop */\nProject: obop, an open-source business platform\nSource: " + GITHUB + "\nStandards: HTML5, CSS3, WCAG 2.2 AA target\nSoftware: VitePress, Vite+, Rust\n");

  // 9. Sanity checks that a deploy depends on.
  for (const f of ["404.html", "index.html", "sitemap.xml", "robots.txt", "llms.txt", "de/index.html"]) {
    if (!existsSync(join(DIST, f))) throw new Error(`build output lacks ${f}`);
  }
}
