import { resolve } from "node:path";
import { defineConfig, type HeadConfig } from "vitepress";
import { LOCALES, messages, type Locale } from "../i18n/index.ts";
import { GITHUB, SITE_URL } from "../scripts/lib/paths.ts";
import { jsonLd } from "../scripts/lib/jsonld.ts";
import { THEME_SCRIPT } from "../scripts/lib/theme-script.ts";

const root = resolve(import.meta.dirname, "..");
const prefixOf = (l: Locale) => (l === "en" ? "" : `/${l}`);

const locales = Object.fromEntries(
  LOCALES.map((l) => [
    l === "en" ? "root" : l,
    { label: messages[l].langName, lang: messages[l].htmlLang, link: `${prefixOf(l)}/` },
  ]),
);

interface Alt {
  lang: string;
  url: string;
}

export default defineConfig({
  srcDir: ".src",
  outDir: "dist",
  title: "obop",
  lang: "en",
  locales,
  appearance: false,
  cleanUrls: false,
  lastUpdated: false,
  titleTemplate: false,
  // A broken internal link fails the build.
  ignoreDeadLinks: [/^\/app\//, /^\/community\//, /^\/status\/?$/],
  markdown: {
    headers: { level: [2, 3] },
    theme: { light: "github-light", dark: "github-dark" },
    lineNumbers: false,
  },
  vite: {
    root,
    server: { fs: { allow: [root, resolve(root, "../../..")] } },
    build: { chunkSizeWarningLimit: 1200 },
  },
  transformPageData(pageData) {
    const fm = pageData.frontmatter as Record<string, unknown>;
    const seo = String(fm["seoTitle"] ?? fm["title"] ?? "obop");
    pageData.title = fm["section"] === "home" ? seo : `${seo} | obop`;
    pageData.description = String(fm["description"] ?? messages.en.description);
  },
  transformHead({ pageData, assets }) {
    const fm = pageData.frontmatter as Record<string, unknown>;
    const head: HeadConfig[] = [];
    const font = assets.find((a) => /geist-latin-wght-normal.*\.woff2$/.test(a));
    if (font)
      head.push([
        "link",
        { rel: "preload", href: font, as: "font", type: "font/woff2", crossorigin: "" },
      ]);
    head.push(["script", {}, THEME_SCRIPT]);

    if (pageData.isNotFound || fm["noindex"] === true) {
      head.push(["meta", { name: "robots", content: "noindex" }]);
      return head;
    }
    const locale = (fm["locale"] as Locale | undefined) ?? "en";
    const m = messages[locale];
    const path = String(fm["pagePath"] ?? "/");
    const url = `${prefixOf(locale)}${path}`;
    const fallback = fm["fallback"] === true;
    const canonical = `${SITE_URL}${fallback ? path : url}`;
    const alts = (fm["alternates"] as Alt[] | undefined) ?? [];
    const mdUrl = fm["mdUrl"] as string | undefined;

    head.push(["link", { rel: "canonical", href: canonical }]);
    for (const a of alts)
      head.push(["link", { rel: "alternate", hreflang: a.lang, href: `${SITE_URL}${a.url}` }]);
    if (alts.length > 0)
      head.push(["link", { rel: "alternate", hreflang: "x-default", href: `${SITE_URL}${path}` }]);
    if (mdUrl) {
      const href = fallback ? `${path === "/" ? "/index" : path.slice(0, -1)}.md` : mdUrl;
      head.push(["link", { rel: "alternate", type: "text/markdown", href }]);
    }
    const ogImage = `${SITE_URL}/og/${fallback ? "en" : locale}${path === "/" ? "/home" : path.slice(0, -1)}.png`;
    const title = pageData.title;
    const description = pageData.description;
    head.push(
      ["meta", { property: "og:site_name", content: "obop" }],
      ["meta", { property: "og:type", content: fm["section"] === "docs" ? "article" : "website" }],
      ["meta", { property: "og:title", content: title }],
      ["meta", { property: "og:description", content: description }],
      ["meta", { property: "og:url", content: canonical }],
      ["meta", { property: "og:locale", content: m.ogLocale }],
      ["meta", { property: "og:image", content: ogImage }],
      ["meta", { property: "og:image:width", content: "1200" }],
      ["meta", { property: "og:image:height", content: "630" }],
      ["meta", { property: "og:image:alt", content: title }],
      ["meta", { name: "twitter:card", content: "summary_large_image" }],
      ["meta", { name: "twitter:title", content: title }],
      ["meta", { name: "twitter:description", content: description }],
      ["meta", { name: "twitter:image", content: ogImage }],
    );
    for (const l of LOCALES) {
      if (l !== locale && alts.some((a) => a.lang === messages[l].htmlLang))
        head.push(["meta", { property: "og:locale:alternate", content: messages[l].ogLocale }]);
    }
    for (const data of jsonLd({
      fm,
      locale,
      path,
      canonical,
      title,
      description,
      githubBlob: `${GITHUB}/blob/main/`,
    })) {
      head.push(["script", { type: "application/ld+json" }, JSON.stringify(data)]);
    }
    return head;
  },
  // The site is static HTML: nothing needs hydrating, so the Vue runtime and the router are not
  // shipped. Interactivity (theme, copy, search, language hint, ASCII field) is a few small plain
  // scripts from /site/. See docs/site/SITE.md.
  transformHtml(code) {
    return code
      .replace(/<link rel="modulepreload"[^>]*>/g, "")
      .replace(/<script type="module"[^>]*src="\/assets\/[^"]*"[^>]*><\/script>/g, "")
      .replace(/<script id="check-dark-mode">[\s\S]*?<\/script>/g, "")
      .replace(/<script>window\.__VP_[\s\S]*?<\/script>/g, "")
      .replace(
        /<link rel="preload stylesheet" href="([^"]+)" as="style">/g,
        '<link rel="stylesheet" href="$1">',
      )
      .replace(/<link rel="preload" href="([^"]+\.css)" as="style">/g, "")
      .replace(/<meta name="generator"[^>]*>/g, "");
  },
  head: [
    ["meta", { name: "color-scheme", content: "light dark" }],
    ["script", { src: "/site/site.js", defer: "" }],
    ["link", { rel: "icon", href: "/favicon.svg", type: "image/svg+xml" }],
    ["link", { rel: "apple-touch-icon", href: "/apple-touch-icon.png" }],
    ["link", { rel: "sitemap", type: "application/xml", href: "/sitemap.xml" }],
    ["link", { rel: "alternate", type: "text/plain", href: "/llms.txt", title: "llms.txt" }],
    ["link", { rel: "author", href: "/humans.txt" }],
  ],
});
