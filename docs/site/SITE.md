# The public website (`web/apps/site`)

The public website of obop: multi-page, statically generated, fully server-rendered, localised in seven languages, with Markdown copies of every page for people's tools and for AI agents. It replaces the single-page landing app (`web/apps/landing`, removed). The sign-up and instances app stays a React SPA under `/app/` (`web/apps/platform`).

- **Status:** working document, 2026-10-16. Written against the code in the repository.
- **Owner decisions it relies on:** D38 (docs on VitePress inside Vite+), D14 (no Node in production: the site is built once and served as plain files), F2 (JS tooling only through `vp` and pnpm).

## Architecture in one picture

```text
content/<locale>/**.md   authored Markdown (en at content/en, translations beside it)
i18n/<locale>.ts         every non-prose string: navigation, footer, home, pricing, app strings
data/apps.ts             hand-written "what it is for" text per app
data/builtin/*.toml      snapshot of the five built-in modules (same format as packages/*.toml)
packages/*.toml          the 22 bundled marketplace packages (read at build time)
docs/MODULES.md          the module guide, included at build time as docs/reference/module-manifest/
git log                  the changelog (data/changelog.snapshot.json where there is no .git)
design/ascii-field.js    the hero's ambient field (copied, not forked)
        |
        v  scripts/assemble.ts
.src/                    the VitePress source tree (generated, not committed)
.generated/              JSON for the theme, the page manifest, the Markdown mirrors
        |
        v  VitePress (.vitepress/config.ts, .vitepress/theme)
dist/                    HTML, one file per route: /docs/agents/mcp/index.html
        |
        v  scripts/postbuild.ts
dist/                    + .md mirrors, search indexes, social cards, sitemaps, robots.txt, llms.txt
        |
        v  deploy/build-site.sh
web/site/                dist/ at the root and the platform app under /app/  (served by the control plane)
```

`vp build` inside `web/apps/site` runs all of that: `vite.config.ts` gives Vite an empty virtual entry and runs the pipeline when it finishes, so the one command CI and the Dockerfile run for every app also builds the site. `vp run build` runs the same script directly and `vp run dev` serves a preview on `http://127.0.0.1:5180` (restart it after editing content).

### Why VitePress 2.0.0-alpha.20

D38 names "VitePress 2 inside Vite+". The newest stable VitePress, 1.6.4 (2025-08), depends on Vite 5; the repository's toolchain is Vite 8. `vitepress@2.0.0-alpha.20` (2026-09-04, more than a month old, so inside the pin policy of D23) depends on `vite ^8.2.1` and `shiki ^4`, and installs against Vite+ 1.0.0 (Vite 8.3.4, Rolldown 1.2.14) without patches. The site builds with zero warnings and renders all 476 pages in about two seconds. The risk of an alpha is accepted because the site is static output we can inspect, the theme is ours (we use almost nothing of VitePress beyond its Markdown pipeline, routing and build), and a fall back to 1.6.4 would need only the Vite pin. Revisit when 2.0 is stable.

### Design decisions

- **Custom theme, no default VitePress look.** `.vitepress/theme` has its own layouts and components on `@obop/ui` tokens, logo, icons and app tiles (rendered to HTML with `@obop/ui/static`). Light and dark come from the tokens (`data-theme` or the system preference). Geist is self-hosted by `@obop/ui`; the latin variable file is preloaded and `font-display: swap` comes from fontsource.
- **No client framework on the page.** The site is static HTML. `transformHtml` removes VitePress's Vue runtime and router, so nothing hydrates. What needs script is a few kilobytes of plain JavaScript in `client/` (served from `/site/`): the theme toggle, copy buttons, search, the language hint, and on the home page only the ASCII field (`/site/home.js`). Everything else is HTML and CSS: the mobile menu and language list are `<details>`, the apps filter is radio buttons and `:has()`, the FAQ is `<details>`. Every page reads and navigates with JavaScript disabled (tested).
- **One Markdown pipeline.** Every page is Markdown (hand-written, or generated from data), so the Markdown mirror of a page is the same text as the page. Pages drawn from data (home, apps index, pricing) have a mirror function in `scripts/lib/mirror.ts`.
- **Trailing-slash routes.** Each route is `.../index.html` (`/docs/agents/mcp/`). Mirrors are `.../mcp.md`.
- **Generated, honest app pages.** For each of the 27 apps: models, fields with kind, data class, personal-data flag and what an agent sees (in clear, masked, withheld), workflows from the actions, permission groups, approval steps, dependencies, install command. Nothing is invented: the structure comes from `packages/*.toml` and `data/builtin/*.toml`; only the "what it is for" paragraphs in `data/apps.ts` are written by hand, and the package description (which carries the limits) is shown verbatim.

## Page, locale and app recipes

### Add a page

1. Create `content/en/<path>.md` (or `<dir>/index.md`) with front matter `title` and `description` (both required; `lead` and `eyebrow` optional). No H1 in the body: the layout draws the title. Docs pages (under `docs/`) get the docs layout; others get the plain page layout.
2. Link to other pages with site-absolute paths (`/docs/agents/mcp/`); the build adds the language prefix. A dead link fails the build.
3. For a docs page add its path to `data/docs-nav.ts` (sidebar order and previous/next).
4. Build. The page appears in every locale, as an English fallback with a visible notice in the languages that have not translated it. It is in the sitemap, `llms.txt`, the search index and the Markdown mirrors without further work.

Tokens you can put on their own line in a page: `%%catalog%%` (the bundled marketplace table), `%%changelog%%`, `%%moduleguide%%` (docs/MODULES.md).

### Add or change an app

The app pages are generated. To add a bundled package: add `packages/<id>.toml` (the Rust catalog in `crates/kernel/obop-pkg/src/marketplace.rs` must list it too), then add an entry to `APP_COPY` in `data/apps.ts` (name, category, English tagline, purpose) and a tagline for it in every `i18n/<locale>.ts` (`apps.taglines`). The build fails if the copy is missing. To change a built-in module, edit `data/builtin/<id>.toml` to match the Rust module.

### Add a locale

1. Add the code to `LOCALES` in `i18n/types.ts` and a file `i18n/<code>.ts` (copy `en.ts`, translate it, set `htmlLang`, `ogLocale` and `langName`).
2. Register it in `i18n/index.ts`.
3. Create `content/<code>/` with the translated pages and a `translation-status.json`.
4. Run `vp run i18n:check <code>` until it is clean, then build. VitePress locale config, sitemaps, hreflang, the language switcher and the Markdown mirrors pick it up from `LOCALES`.

RTL is out of scope; the CSS uses logical properties so adding it later is a layout review, not a rewrite.

## Translation workflow

**All translations are machine-drafted** (written by a language model, 2026-10-16) and have not been reviewed by a native speaker. They are marked as such in `content/<locale>/translation-status.json`, which maps each translated page to `machine-drafted` or `reviewed`. The build fails if a translated page has no entry, or an entry has no page.

What is translated: all interface strings (navigation, footer, buttons, banners, search), the home page, the apps index, the headings and labels of every app page and the one-line summary of each app, pricing, Getting started, the Agents overview, Create an agent token and Connect over MCP, Platform and About. The longer "what it is for" paragraphs, package descriptions and field descriptions on app pages are English, marked with `lang="en"`, because they come from the manifests. Every other page falls back to English with a banner ("This page is not translated yet") and is not listed in `hreflang` alternates or the sitemap of that language; its canonical URL is the English page.

To review a language: read `i18n/<locale>.ts` and the files in `content/<locale>/`, fix them, and change the page's entry in `translation-status.json` to `reviewed`. To translate another page: copy `content/en/<path>.md` to `content/<locale>/<path>.md`, translate it (keep code blocks, links and front matter keys; `vp run i18n:check` enforces this), and add its path to `translation-status.json`.

The root `/` never redirects by `Accept-Language`, so crawlers always see the English page. If the browser's preferred language is one of ours and differs from the page, a small dismissible bubble at the bottom of the page (fixed position, so nothing shifts) offers the translated version of the same page. Dismissal is remembered in local storage, not a cookie.

## SEO and agent-readiness checklist

Per page: unique `<title>` and description, canonical URL, `hreflang` alternates and `x-default` (translated variants only), Open Graph and Twitter cards with a generated 1200x630 image per page and language, `<link rel="alternate" type="text/markdown">`, JSON-LD (Organization, WebSite with SearchAction, SoftwareApplication for obop and for each app, FAQPage on the home and pricing pages, BreadcrumbList, TechArticle for docs), one `h1`, ordered headings, landmarks, `lang`.

Per site: `sitemap.xml` (an index of one sitemap per language plus `/community/sitemap.xml`), `robots.txt` (everything allowed, AI crawlers named), `llms.txt` and `llms-full.txt`, `/.well-known/security.txt`, `humans.txt`, Markdown mirrors, `404.html`, a favicon and a touch icon. The full mapping to Vercel's agent-readiness checks is in [AGENT-READINESS.md](AGENT-READINESS.md).

Core Web Vitals: no render-blocking script, one CSS file, the latin Geist variable font preloaded with `font-display: swap`, explicit sizes on all icons and tiles, nothing that shifts after load (the language hint is fixed-position), images are inline SVG. The home page loads two small scripts and the ASCII field; other pages load one.

Checks that run in CI are in `web/packages/e2e/tests/site.spec.ts` (no-JavaScript rendering, apps filter, docs navigation, search, language switch and hint, 404, sitemap, robots, llms, mirrors, security.txt, JSON-LD, headings, landmarks, labels, no horizontal scroll). `vp check` and `vp build` in the app must pass without warnings.

## Server requirements

The control plane serves `web/site` with a plain static file server. The site works with that today, because every route exists as a file. These are the changes it needs, in order of importance:

1. **A real 404.** For a path with no file, answer status **404** with the body of `/404.html` (`Content-Type: text/html`). Today unknown paths fall back to `index.html` with status 200, which makes every typo a "soft 404" that search engines and agents misread. Do this for the website only, not for `/app/` (the SPA needs its fallback to `/app/index.html`).
2. **Directory routes.** `/docs/agents/mcp` (no slash) should redirect with 301 to `/docs/agents/mcp/`; `/docs/agents/mcp/` serves `index.html`. (`ServeDir` already serves the directory index; the redirect is the addition.)
3. **Markdown negotiation.** For a page URL (`/docs/agents/mcp/`) with `Accept: text/markdown`, answer with the file at the same address with `.md` instead of the trailing slash (`/docs/agents/mcp.md`; the home page is `/index.md`, `/de/` is `/de/index.md`) and `Content-Type: text/markdown; charset=utf-8`. Add `Vary: Accept` to the HTML responses of those URLs, and `Link: </docs/agents/mcp.md>; rel="alternate"; type="text/markdown"` is optional because every page already has the `<link>` in its head.
4. **Content types.** `.md` as `text/markdown; charset=utf-8`, `.txt` as `text/plain; charset=utf-8`, `.xml` as `application/xml`, `.webmanifest` not used, `.png` and `.svg` as usual.
5. **Caching.** `/assets/*` (hashed file names) and `/site/*` may be `Cache-Control: public, max-age=31536000, immutable` if the files under `/site/` get hashed names later (they do not today: give them a short max-age, for example 3600). HTML: `no-cache` or a short max-age with revalidation. `ETag` or `Last-Modified` on everything.
6. **Compression.** gzip or brotli for HTML, CSS, JS, JSON, XML, SVG and text.
7. **Reserved paths.** The forum (`/community/*`) and the status page (`/status*`) are mounted by the control plane before the static files. The website therefore keeps its own pages for them at `/about/community/` and `/about/status/`, which link to the live ones. `/community/sitemap.xml` must exist for the sitemap index. Do not serve `/app/*` from the website.
8. **Security headers** (optional, good practice): `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, a `Content-Security-Policy` of `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; base-uri 'self'; frame-ancestors 'none'`. The site uses two small inline scripts (the theme setter in `<head>`, the language-hint data block) and inline `style` attributes on app tiles.
9. **Origin.** Canonical links, the sitemap and social cards need the public origin. Set `OBOP_SITE_URL` when building (default `https://obop-platform.sliplane.app`, the known Sliplane address); a custom domain needs a rebuild with that variable.

## Build integration

- `deploy/build-site.sh` builds the site and the platform app and assembles `web/site`.
- The `Dockerfile` web stage builds the same with `pnpm --filter @obop/site exec vp build`. The site reads `packages/`, `design/` and `docs/MODULES.md`, so the stage copies them (the `.dockerignore` re-includes `docs/MODULES.md`). There is no `.git` in the image, so the changelog comes from `data/changelog.snapshot.json`; refresh it with `vp run changelog:snapshot` (in `web/apps/site`) before a release.
- **CI.** `.github/workflows/ci.yml` loops over `backoffice platform landing`. `landing` no longer exists: change the two loops to `backoffice platform site`. Nothing else in CI changes; `vp check` and `vp build` work in `web/apps/site` like in the other apps.

## Maintenance notes

- The five built-in modules are described twice: in Rust and in `data/builtin/*.toml` (a hand-kept snapshot). A test that compares the two would remove the drift risk.
- Social cards are rasterised with `@resvg/resvg-js` (prebuilt binaries, no native build). Geist is a WOFF; the build unwraps it to TrueType in memory for resvg.
- Search is a small script over JSON indexes (`/search/<locale>.json`), built from the page text at build time. It is plain substring scoring: good enough for a few hundred pages, not a full-text engine.
- Pages in languages other than English are 7 x 68 = 476 routes; the fallbacks are copies of the English text, generated, not stored.
