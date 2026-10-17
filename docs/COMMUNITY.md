# The community forum and the status page

- **Status:** working draft, 2026-10-16. Written against the code in `crates/kernel/obop-community` and `crates/kernel/obop-http/src/community.rs`, not a design target.

The control plane (`obop platform serve`) serves two dynamic, server-rendered pages next to the static marketing and docs site: a **forum** under `/community` and a **status page** under `/status`. Both are plain HTML (no client-side framework, no script except one opt-in line on the status page), carry full SEO and agent-readiness metadata, and use the control-plane accounts. Their routes take priority over the static site for their prefixes; `/api/platform/*` and the static fallback are unchanged.

## Layout in the code

| Piece | Where |
|---|---|
| Logic, rendering, tests (no web framework) | `crates/kernel/obop-community` |
| axum adapter, the only place the two meet (D29) | `crates/kernel/obop-http/src/community.rs` (`obop_http::community::router`) |
| Wiring, checker start, CLI | `crates/obop/src/main.rs` (`platform_serve`, `status_probes`, `status_incident`) |

`Community::handle(&Req) -> Resp` is the whole interface; `Req` and `Resp` are small framework-neutral structs. The router is merged into the platform router: `platform_router(api, static_dir).merge(community::router(community))`.

## Configuration

| Variable | Meaning | Default |
|---|---|---|
| `OBOP_PLATFORM_ADMINS` | Comma-separated emails. These accounts are staff: moderate the forum, post in Announcements, run `/status/admin`. | none |
| `OBOP_PUBLIC_URL` | Public origin for canonical links, Open Graph, feeds, sitemaps and JSON (`https://obop.example`). Set it in production. | taken from the request `Host` |
| `OBOP_STATUS_INTERVAL_SECS` | Seconds between status checks (minimum 5). | `60` |
| `OBOP_CELL_ADMIN_URL` | Cell PostgreSQL admin URL. When set, a `SELECT 1` check adds the **Cell database** component. | unset (component omitted) |
| `--db` / `OBOP_DATABASE_URL` | Storage for everything below (SQLite, PostgreSQL or `memory:`), shared with the platform. | `sqlite://obop-platform.db` |

TLS terminates at the proxy: send `X-Forwarded-Proto: https` so cookies get `Secure`.

## Forum

### Accounts and sessions

- Reading is public. Posting needs an account. Accounts are the control-plane accounts of `obop-auth` (Argon2id). `/community/signup` creates one (12+ character password), `/community/login` accepts any existing control-plane account.
- A successful login sets the cookie `obop_session`: 256 random bits as hex, `HttpOnly`, `SameSite=Lax`, `Secure` when the request came over HTTPS, 30 days. Only its SHA-256 is stored (`community_session`); logout revokes it server-side. The JSON bearer tokens of `/api/platform/login` are separate and untouched.
- **CSRF.** Every POST needs a `csrf` field. For logged-in users it is `SHA-256("obop-csrf-v1:" + session token)`, bound to the session; for the anonymous login and signup forms it is the value of the `obop_csrf` cookie (double submit). Comparison is constant time; a missing or wrong token, or a cross-origin `Origin`, gives `403`.
- Login attempts use the lockout of `obop-auth` (5 failures, one minute, `429`).
- Display names: 3 to 24 of `A-Za-z0-9_-`, unique (case-insensitive), no name containing `obop`, `admin`, `staff` or `moderator` unless the account is staff. If none is chosen it is derived from the email local part. Email addresses are never rendered.

### Pages

| URL | Content |
|---|---|
| `/community` | categories with counts and latest activity, pinned announcements, recent threads |
| `/community/c/{slug}` | threads of a category, pinned first, then newest activity, 25 per page (`?page=`) |
| `/community/t/{id}-{slug}` | thread with posts, 30 per page; a wrong slug is a `301` to the canonical URL |
| `/community/c/{slug}/new`, `/community/t/{id}/reply`, `/community/p/{id}/edit` | forms (also POST targets) |
| `/community/u/{name}` | profile: post count, join date, recent threads |
| `/community/search?q=` | case-insensitive substring over titles and bodies, 20 per page, `noindex` |
| `/community/feed.xml`, `/community/c/{slug}/feed.xml` | Atom, latest 30 threads |
| `/community/sitemap.xml` | sitemap with `xhtml:link` language alternates |
| `/community/login`, `/signup`, `/logout` | accounts |
| `/community/mod/log` | staff only: audit log |
| `/community/assets/app.css` | the one stylesheet (immutable, versioned with `?v=`) |

Categories are seeded on first run: General, Help, Show and tell, Modules, Agents and MCP, Self-hosting, Feature requests, Announcements (staff only). Threads and posts are JSON documents in `community_thread` and `community_post`; the forum is held in memory as a cache and written through; since D59 several control-plane processes may share one database: a write takes a database lock, reloads the copy if another process changed anything (a shared version number) and bumps the version, and a read reloads when the number moved (checked at most every 250 ms).

### Writing rules

Titles at most 140 characters, bodies at most 20 000, request bodies at most 64 KiB (`413`). Per account and hour: 10 posts and 3 new threads (staff exempt, `429` with `Retry-After`). Accounts younger than 24 hours may include at most 2 links per post. A hidden `website` field is a honeypot. A post can be edited by its author for 15 minutes.

### Markdown

Posts are rendered with `pulldown-cmark` and then sanitised with `ammonia` (strict allow-list): raw HTML is shown as text, images become plain links, only `http`, `https` and `mailto` links survive, every link gets `rel="nofollow ugc noopener"`, code blocks keep only a `language-*` class, headings are demoted below the page `<h1>`.

### Moderation

Staff (`OBOP_PLATFORM_ADMINS`) see a bar on every thread and a Hide button per reply. Actions: pin/unpin, lock/unlock, hide/unhide (thread or reply), move to another category, and permanent removal. Each is written to the audit log (`community_audit`), shown at `/community/mod/log`.

- A **hidden** thread or reply stays a real `200` page for everyone else, says "This content was removed by a moderator." and never contains the text (the URL slug is replaced, the page is `noindex`, and it leaves lists, search, feeds and sitemaps). Staff still see the content, flagged.
- A **removed** thread (tombstone) answers `410` for everybody and its text is erased from storage.
- A locked thread takes no replies from members.

### SEO and agent readiness

Every page has a unique `<title>`, meta description, canonical URL, Open Graph and Twitter tags, `hreflang` alternates for the seven languages plus `x-default`, and JSON-LD: `BreadcrumbList` everywhere, `CollectionPage`/`ItemList` on lists, `DiscussionForumPosting` with `comment`s on threads, `ProfilePage` on profiles. Real status codes: `404` (with the category list and a search box), `410`, `301`, `403`, `405` (with `Allow`), `413`, `429`. `noindex` on search, login/signup, forms and hidden or locked-and-unanswered threads. Paginated pages carry `rel="prev"`/`rel="next"`.

**Every page is also Markdown** at the same URL, with `Accept: text/markdown` or `?format=md` (`Vary: Accept`). Anonymous pages are cacheable (`Cache-Control: public, max-age=60, stale-while-revalidate=300`, `ETag`, `Last-Modified`, `304`); anything personalised is `private, no-store`. Security headers: a strict CSP (`default-src 'none'`, own styles and scripts only), `nosniff`, `X-Frame-Options: DENY`, referrer policy.

### Accessibility

Landmarks (header, nav, main, footer, search), a skip link, one `<h1>` per page, labelled form fields with `aria-describedby` help and `role="alert"` errors, visible `:focus-visible` outlines, targets of at least 40 px, light and dark colours from `prefers-color-scheme` meeting AA contrast, no motion beyond a colour fade (disabled for `prefers-reduced-motion`), state never conveyed by colour alone.

## Status page

`obop platform serve` starts a checker thread. Every `OBOP_STATUS_INTERVAL_SECS` it probes:

| Component | Check |
|---|---|
| Website and docs | an internal `GET http://127.0.0.1:{port}/` (only when `--static` is given) |
| Control plane API | storage write/read/delete round trip |
| Community | storage round trip on the community table |
| Provisioning | `sliplane --version` for `--provisioner sliplane` (5 s limit); operational for `local` and `fake` |
| Cell database | `SELECT 1` over `OBOP_CELL_ADMIN_URL`; omitted when unset |
| Customer instances | `HEAD /` with a 5 s timeout on every `running` instance, in parallel |

The instance probe is hardened: only the stored URL of a `running` instance is requested; it must be exactly `https://<dns name>` (no userinfo, port, path, IP literal or `http`), redirects are not followed, and every address the name resolves to must be public. Only instance **names** appear on the page: the percentage up and the degraded instances. All up is operational, some down is degraded, most down (of at least three) is a major outage.

Results are kept as daily counters per component for 100 days (`status_day`) plus a ring of recent samples (`status_recent`). Uptime is the share of checks that found the component available (degraded counts as available) over 24 hours, 7 days and 90 days. A day in the 90-day bar is "outage" when at least 5% of its checks failed, "degraded" when any failed or were degraded, "operational" otherwise, "no data" without checks; each day is a list item with a text alternative and a distinct pattern, so colour is not the only signal.

**Automatic incidents.** Three consecutive failed checks of a component open an incident (`major`); three consecutive successful checks resolve it.

**Manual incidents and maintenance.** Staff use `/status/admin` (forms, CSRF protected) or the CLI, which works directly on the database given with `--db`:

```bash
obop status incident create --title "Slow logins" --impact major --components api,community --message "Investigating"
obop status incident update 3 --status identified --message "Found the cause"
obop status incident resolve 3 --message "Fixed"
obop status incident list
```

Impact is `none|minor|major|critical`, status `investigating|identified|monitoring|resolved`, components `website|api|community|provisioning|celldb|instances`. An open incident raises the state of its components; a maintenance window (staff form) shows "Under maintenance" while it runs.

### Machine endpoints

All answer with `Access-Control-Allow-Origin: *`, `Cache-Control: public, max-age=30`, `ETag`, `Last-Modified` and `304`, and use the Atlassian Statuspage v2 shapes (so existing status widgets and clients work):

- `/status/api/v2/summary.json`: `page`, `status` (`indicator` `none|minor|major|critical`, `description`), `components`, unresolved `incidents` with `incident_updates` (newest first) and `scheduled_maintenances`
- `/status/api/v2/status.json`, `/status/api/v2/components.json`, `/status/api/v2/incidents.json` (latest 50, resolved included)
- `/status/history.atom`: incident history as Atom
- `/status` and `/status/incidents/{id}` also as Markdown (`Accept: text/markdown` or `?format=md`)

The page uses no `<meta http-equiv="refresh">`; the "Turn on auto refresh" link (`/status?refresh=60`) adds one external script line (`/status/assets/refresh.js`) that reloads after a minute.

## Languages

Chrome (navigation, labels, buttons, errors, dates, numbers) is available in English, German, French, Polish, Finnish, Swedish and Spanish. The language is chosen by `?lang=` (which also sets the `obop_lang` cookie), then the cookie, then `Accept-Language` (with q-values), then English. Responses carry `<html lang>`, `Content-Language`, `hreflang` alternates and `og:locale`. User-written content is never translated and its language is not guessed.

To add or change a string, edit the `CATALOGUE` table in `crates/kernel/obop-community/src/i18n.rs`: one row per key with seven columns in the order of `Lang::ALL` (en, de, fr, pl, fi, sv, es); `{name}` placeholders must appear in every column (a test enforces completeness and matching placeholders). Category names and descriptions are rows `cat.{slug}.name` and `cat.{slug}.desc`; component names are `comp.*`. To add a language, extend `Lang`, add a column to every row, a month list to `MONTHS` and separators to `group_sep` and `decimal_sep`.

## For the static site

- `Community::sitemap_urls(base) -> Vec<SitemapUrl { loc, lastmod }>` returns every URL of the forum and `/status`; the same list is served at `/community/sitemap.xml`. List `/community/sitemap.xml` in the main sitemap index.
- The stylesheet uses `--accent`, `--panel`, `--line`, `--fg`, `--muted` and `--radius` (names from the backoffice); re-point the `:root` blocks in `assets.rs` to the shared design tokens when they land.
- The header and footer (`layout.rs`) link to `/`, `/docs/`, `/community`, `/status`; keep the static pages' navigation consistent.

## Known limits

- A post made on one control-plane process shows on the others within the version check (250 ms); the status checks run on one process per interval.
- No per-IP rate limit on signup or login beyond the account lockout; put one in the proxy.
- DNS is resolved just before the instance check, not pinned to the connection.
- Raw check results older than the ring (about 24 hours) exist only as daily counters.
- Instance records are read straight from the `platform_instance` table; `obop-platform` has no listing API for all instances.
