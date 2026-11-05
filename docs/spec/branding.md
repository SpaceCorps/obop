# Instance branding: one record that names the product where an end user sees it (PROPOSED)

> Status: PROPOSED. The direction in section 1 is the owner's, relayed and not yet in the decision register. Everything else here is a proposal for review: nothing is built, nothing was run, and no owner ruling backs any part of the design. The register entries it needs (decision D94, question Q71) are in section 15.
>
> Written against: `main` = origin/main at 12a59a79 (09861505 plus #62 and #64; the cited lines were re-opened after the merge). Citations are `main:path:line`; the line was opened on that ref and says what the sentence claims. `K/` is `crates/kernel/`, `BO/` is `web/apps/backoffice/`, `UI/` is `web/packages/ui/`. "Audit row" ids (SP-nn server pages, OT-nn other server surfaces, BO-nn back office, PA-nn phone app, HP-nn host plan) come from a branding audit of 2026-11-04 that is a working paper outside the repository; they are kept so a reviewer can map this document to it, and nothing here depends on reading it.
>
> Marks: "verified" means the line was opened on `main` and says what the sentence claims. "Not verified" means it was not opened or cannot be opened from the repository. A page text quoted from a render comes from a separate local render check of the same ref (three throwaway instances, one browser); this document did not repeat it.

## 0. Summary

A customer who runs an obop instance for its own people can show them its own name, not "obop". One record, `instance_brand`, holds the name, the logo, a sign-in title per language, a sign-in message per language and one switch. Every place that shows the product name to an end user reads that record through one function. An instance that sets nothing renders byte for byte as today and the default product name stays obop.

- One record, a typed command surface (CLI, REST, MCP, a tab in Customize), changed only by a person who holds `platform.admin` (sections 2 and 3).
- Stored as a record, not as environment variables, because it is a command, it has a logo, several server processes must agree and a change must be audited (section 4).
- It defers to what exists: the instance theme owns the accent, the `site.*` keys stay the explicit override of the public site, `OBOP_MFA_ISSUER` stays an explicit override of the authenticator label (section 5).
- Server-rendered pages take the record on every request. The back office, which must show the name before anyone has signed in, gets a small public subset injected into its page, the way `OBOP_DEFAULT_LANGUAGE` is today (sections 4 and 6).
- Five sentence families of the OAuth pages name obop, and German, Finnish and Swedish attach the name to compounds and case endings. With a name set they switch to neutral wording that names no product, in all seven languages (section 7).
- The OAuth and signing pages keep their content security policy. The logo there is inline SVG built by the server from a strict subset, never an image or a style (section 8).
- What the record cannot touch is fixed and listed: the security statements of the consent page, the wire names, the licence and NOTICE text, any attribution (sections 10 and 11).
- Delivery is four slices, S1 to S4, sized in section 12. Tests, including a scan that fails when a literal `obop` is added to a page that should use the record, are in section 13.

## 1. The direction, and what is not asked

Owner direction of 2026-11-04 and 2026-11-05 (a card answer, relayed by the setup session; not a quotation and not yet entered in `docs/DECISIONS.md`): the end users of the thing built for MOHV must see no obop branding, and that is built into obop as a general white-label feature, not as a one-off. There is one instance branding record that every surface reads; MOHV only sets values. The values for MOHV are the name `MOHV` and, for now, a plain "M" as the logo. The phone app is called MOHV on the phone, with application id `se.mohv.app` and test id `se.mohv.app.test`; "Mileage" stays the module name inside the host.

Not asked, and not touched by this document or by any slice:

1. Any "Powered by obop" line or other attribution. None exists on screen today (verified: a case-insensitive grep for "powered by" and "x-powered-by" over the Rust, TypeScript and HTML of `crates/`, `web/apps` and `web/packages` finds nothing).
2. `LICENSE`, `NOTICE` and the attribution they carry (Apache-2.0 section 4(d)).
3. The name-use question (Q2, OR-5): counsel's opinion is due 2026-12-15. This record lets an operator replace the name where an end user sees it; it says nothing about who may use "obop".
4. Wire-level names (section 11).

## 2. The record

One model, `instance_brand`, in the built-in module `platform` (the Customize app) beside `setting` (`K/obop-pkg/src/installer.rs:2418`) and `sso_provider` (`K/obop-pkg/src/sso.rs`). One row, id `instance`. No row means no brand. In this document "with a row" means a row whose `name` is not `obop` (any case) or that has an artwork (`Brand::is_custom`); a row that names `obop` and has no artwork changes only `hide_product_docs`, so it cannot replace the obop logo with a letter tile. Every field has data class `none` (the code's `DataClass::None`, "explicitly not personal data", the class `setting.value` has; CONVENTIONS 5 lists no row for it). Every field is readable without signing in, by design, so none may hold personal data; the field descriptions and the form say so. `<lang>` is each of `en`, `de`, `fr`, `pl`, `fi`, `sv`, `es` (`K/obop-oauth/src/page.rs:16`).

| Field | Kind and limit | Meaning |
|---|---|---|
| `name` | text, 1 to 40 | The product name. Required when the row exists. Letters, digits, space and `- . , & ' + _ ( ) !`; the first character is a letter or digit. Nothing else: no markup, quotes, slash, colon or `@` (so no URL or address can pass as a name) |
| `slug` | text, up to 30 | Lowercase `a-z`, `0-9`, `-`. Empty means derived from `name` (other runs become `-`, ends trimmed, `instance` if nothing is left, so a name with no Latin letters never falls back to `obop`). The stem of the recovery-codes file and the MCP `serverInfo.name` |
| `title_<lang>` (7) | text, up to 60 | The sign-in title in that language: the line beside the logo on the sign-in screens and the tab title before sign-in. Empty means `name`. Same character rule as `name` |
| `message_<lang>` (7) | text, up to 240 | One plain-text message under the heading of the sign-in screens in that language. Empty means none. No `<`, `>`, control, bidirectional or zero-width characters; shown escaped, no links |
| `logo_svg` | longtext, up to 16 KiB in | The logo and the tab icon, one artwork. Input is an SVG file; what is stored is the canonical text the parser of section 8 re-wrote, never the input. Empty means the generated tile below |
| `logo_hash` | text, read only | SHA-256 (first 16 hex digits) of what is served; the cache key |
| `hide_product_docs` | bool | Hide the Brand book and the Guide in Customize, the account menu and the command palette (section 6) |

No colour field: the accent stays the instance theme's (section 5). No `logo` for dark mode: one artwork, so a logo that must work on both light and dark backgrounds uses explicit colours (section 14).

**The generated tile.** With a name and no `logo_svg`, the server draws a tile: the first letter or digit of the name, uppercased, on a rounded square in the accent, with the contrast rule the public site already applies (`K/obop-site/src/seo.rs:56-72`). That is the "M" placeholder for MOHV, and no file is needed for it. Swapping to the real logo later is one command with one file: `obop admin brand configure --logo-file logo.svg`.

## 3. Commands, who may change it, audit

Per CLAUDE.md "agentic first", every capability is a typed command on all surfaces. The shape follows `sso_provider` (`K/obop-pkg/src/sso.rs`), a one-row configuration model with `get`, `configure` and `clear`:

| Command | CLI (the built `admin` area) | REST and MCP | Effect |
|---|---|---|---|
| `instance_brand.get` | `obop admin brand status` | `/api/instance_brand/get`, tool `instance_brand__get` | Returns the row; read group `*` (any signed-in principal), because the values are public anyway |
| `instance_brand.configure` | `obop admin brand configure [--name N] [--slug S] [--title LANG=TEXT]... [--message LANG=TEXT]... [--logo-file F] [--hide-product-docs]` | `/api/instance_brand/configure`, `instance_brand__configure` | Validates and replaces the whole row. A refused value names the field and the rule |
| `instance_brand.clear` | `obop admin brand clear` | `/api/instance_brand/clear`, `instance_brand__clear` | Deletes the row; every surface is as today |

Who: group `platform.admin` (`K/obop-commands/src/lib.rs:52`) and a person. The command is refused for an agent token, a scoped token and the narrowed token of a mobile app (`Principal::is_person`, the check `sign_link.create` makes at `K/obop-commands/src/sign/link.rs:52`), and an agent does not even propose it (the rule `person_only` of `sso_provider`, `K/obop-pkg/src/sso.rs:102`): a name and a logo decide what a person believes they are signing in to. Reading is open to any principal. At a terminal `obop admin brand` acts on the database as the operator, as `obop site set` does (`crates/obop/src/main.rs:1349-1362`): a person with the host, not a token. The back office tab (section 6) calls these commands and nothing else, so there is no UI-only path, and the parity gate of D80 applies to all three commands.

Audit: each `configure` and `clear` writes a `security_event` of kind `brand_configured` or `brand_cleared` with the actor and, for the name, the old and the new value (a name is not a secret; the logo is recorded as its `logo_hash`). Both kinds are added to the list in the description of `security_event.kind` (`K/obop-pkg/src/installer.rs:2443`), which `K/obop-http/src/audit_kind_tests.rs` holds to the code.

## 4. Where it is stored, and how each surface reads it

**Record, not environment.** The existing branding-like settings are environment variables read once at start (`OBOP_MFA_ISSUER`, `K/obop-auth/src/mfa.rs:99-101`; `OBOP_DEFAULT_LANGUAGE`, `K/obop-http/src/shell.rs:1-9`). That does not fit here: (1) an environment variable is not a command, and every surface must have one; (2) a logo of 16 KiB does not belong in an environment variable; (3) several server processes on one PostgreSQL database must agree at once without a restart, and the record is read per request; (4) a change must be audited and the environment has no author. Precedent for a record read per request exists: `Auth::security_setting` reads `obop_setting/instance` each time (`K/obop-auth/src/mfa.rs:146-150`) and `site_settings` does the same for the public side (`K/obop-commands/src/public.rs:746-760`). No new environment variable is added, and `OBOP_MFA_ISSUER` and `OBOP_DEFAULT_LANGUAGE` keep working (section 5).

**Not inside `obop_setting/instance`.** That row is one JSON text that `setting.update` replaces whole for any principal that holds `platform.admin`, with no person-only check (`K/obop-pkg/src/installer.rs:2614-2626`), and its one field is class `none` with no per-field rule. The brand needs per-field validation and a person-only guard, so it is its own model. The theme stays where it is.

**One reader.** `obop_commands::brand::current(&dyn Backend) -> Brand` reads the row and runs the same validation as the command on every field (defence in depth: a row written by a restore, an import or an older binary is checked again on read, and a field that fails falls back to the default for that field, never rendered raw). `Brand` lives in a new leaf crate `obop-brand` (the validation, the slug, the SVG parser, the tile, the neutral-wording switch; no I/O, no external dependency), as `obop-values` is a leaf crate. `Brand::default()` is name `obop`, no artwork, no titles, no messages, neutral wording off, and so is any row that is not custom; every consumer asks `Brand` for the name and so takes one code path with or without a row. A failed read renders the default brand and writes one log line; the rest of the page fails or not as it does today. The shell, the OAuth handlers, the signer door, the connector return page, `mfa`, the passkey context builders and the MCP server call `brand::current` and no other code reads the row (a scan, section 13).

**Pages before sign-in.** The back office shows the sign-in, second-factor, enrolment and forced-password screens before the settings can be read, and no pre-sign-in route carries a setting today: "no other setting is public" (`K/obop-http/src/shell.rs:3-8`; `/api/meta` and `/api/commands` answer 401 without a token, from the render check). The same mechanism carries the brand. The shell already rewrites `index.html` per request and is served `Cache-Control: no-cache` (`shell.rs:10-12`); with a row it also (1) replaces the first `<title>…</title>` with the escaped `name`, (2) replaces the icon link at `BO/index.html:7` with `<link rel="icon" href="/api/brand/logo.svg?v=<logo_hash>" type="image/svg+xml">`, and (3) adds `<meta name="obop-brand" content="…">` whose value is one JSON object, attribute-escaped by the existing `shell::attribute`: `name`, `slug`, `titles` (the non-empty ones), `messages`, `logo` (the hash, or absent for the tile), `accent` (the theme's, validated `#rrggbb`) and nothing else. The page reads it before its first render, as it reads `obop-default-language`. `hide_product_docs` is not in it; the signed-in app gets it from `instance_brand.get`. A shell that cannot take the rewrite (no `<head>`, no `<title>`) is served as it is, as the language tag is today (`shell.rs`, `inject`).

**The logo route.** `GET /api/brand/logo.svg` (public, like `/api/openapi.json`) serves the stored canonical text or the tile and nothing else; it takes no path or id, so it is not a file server. Headers: `Content-Type: image/svg+xml`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; style-src 'none'; sandbox`, `Cache-Control: no-cache`, `ETag` = the hash. No row: 404 (today there is no such route).

## 5. What it absorbs, what it defers to

One rule: the record is the one place to set the product name and its artwork; the older settings stay and still win where they are explicit, so nothing an operator already set changes meaning. The record becomes the default for the authenticator issuer and for the public site's title, logo and tab icon. It defers to the instance theme for the accent and has no colour of its own. `OBOP_MFA_ISSUER` and the `site.*` keys are kept as explicit overrides.

| Setting today | With the record | Order |
|---|---|---|
| `OBOP_MFA_ISSUER` (label prefix and `issuer=` of the authenticator entry, one value: `K/obop-auth/src/totp.rs:117-123`, `mfa.rs:812-813`) | Stays. The issuer is the variable if set, else `name`, else `obop` | env, record, default |
| Instance theme (`theme.accent` and the rest, `BO/src/api.ts:589-594`; set in Customize, Theme) | Owns the accent. The record has no colour. Its accent value (not personal data) joins the public subset so the sign-in screens and the tile use it | theme, then the back office default violet `#6d3fd0` (`UI/src/tokens.css:76`) |
| `site.title`, `site.logo_text`, `site.logo_url`, `site.accent`, `site.footer_text` (`K/obop-site/src/settings.rs`, set with `obop site set`) | Stay the explicit settings of the public site, since a shop can be named differently from the product. When a key is unset the site falls back to the record: `title` ← `name`; `logo_url` ← `/api/brand/logo.svg` when the row has an artwork; `accent` ← the theme's; the favicon at `/favicon.svg` ← the record's artwork. `footer_text` has no fallback | site key, record or theme, today's default (title `My Shop`, accent teal `#0f766e`, `K/obop-site/src/theme.rs:226`) |
| `OBOP_DEFAULT_LANGUAGE` | Unchanged. It is the language of the back office index. The signer page uses it as its last fallback (`K/obop-http/src/sign.rs:83`); the OAuth pages do not (`K/obop-oauth/src/page.rs:57` falls to English) and this document does not change that | unchanged |
| `OBOP_PUBLIC_URL` | Unchanged. It decides the host, so the passkey `rp.id` and the signing link; the host name is outside this record (section 11) | unchanged |
| Instance setting `apps.names` (launcher names) | Unchanged; it names apps, not the product | unchanged |

## 6. Surfaces

"Today" cites `main`. "With a row" is the proposal. Rows marked (a) are admin-only; the MCP, discovery, OpenAPI and Slack rows are seen by staff who connect a client, by developers and by the members of a Slack channel; all other rows are what an end user sees.

| Audit row | Surface | Today | With a row | Slice |
|---|---|---|---|---|
| SP-01 | OAuth pages (sign-in, second factor, consent, error) | `<p class="brand">obop</p>` (`K/obop-oauth/src/page.rs:182`) | `.brand` holds the inline logo, then `title_<lang>` or `name`, escaped | S1 |
| SP-02..06 | Five catalogue keys name obop (`login.lead` twice, `consent.who_body`, `consent.f2`, `consent.who_body_mobile`, `…_loopback`: `page.rs:453,470,482,484,488`; 5 keys, 35 cells, 42 occurrences, counted by script) | "this obop instance", "your obop account", "obop cannot verify who runs it", "approve them in obop" | Neutral twins (section 7) | S1 |
| OT-01 | Signer page, every state | `<p class="brand">obop</p>` (`K/obop-http/src/sign_page.rs:67`) | As the OAuth `.brand`. The signer texts name no product (`K/obop-http/src/sign_text.rs` has no product word outside code comments) and eight of them (nine for a request that shows the history of the file) are hashed into the consent id (docs/ISSUES.md O2); they are not touched and neither the name nor the logo enters the id | S1 |
| SP-08, OT-03 | Tab icon of the OAuth and signer pages | No icon link (`page.rs:174-182`); the browser asks `/favicon.ico` and gets the public site's tile or the back office index (render check) | With an artwork or a name: `<link rel="icon" href="/api/brand/logo.svg?v=…">`, and `img-src 'self'` added to the two policies in that case only | S1 |
| OT-06 | Passkey relying-party name | `rp_name: "obop".into()` in two builders (`K/obop-http/src/passkeys.rs:65`, `K/obop-http/src/oauth.rs:813`); emitted only in the registration options (`K/obop-auth/src/passkey.rs:619`), never in a sign-in | Both builders use `name`. `rp.id` stays the host. Stored passkeys keep their `rp_id`, so none is orphaned | S1 |
| OT-05 | Authenticator entry | Label prefix and issuer both `OBOP_MFA_ISSUER`, default `obop` (`mfa.rs:99-101`, read once at start) | Section 5; read per enrolment so a record change applies. Entries already enrolled keep their label | S1 |
| OT-04 | Public site and shop | Clean; defaults from `site.*` (`settings.rs`, default title `My Shop`) | Section 5 fallbacks | S1 |
| SP-11 (a) | Connector return page | "Back to obop", "Taking you back to obop." (`K/obop-http/src/connectors.rs:111,162`), English only | `name`; still English only (this is the admin's own return page) | S1 |
| BO-01, BO-02 | Back office tab title and icon | `<title>obop</title>`, `<link rel="icon" href="/favicon.svg">` (`BO/index.html:7-8`); nothing sets `document.title` | Section 4. After the language is known the page sets `document.title` to `title_<lang>` or `name` on the sign-in screens and to `name` after sign-in | S2 |
| BO-04..09 | Logo | `Logo` with accessible name `obop`: sign-in three places (`BO/src/SignIn.tsx:186,206,223`), forced screens (`BO/src/Security.tsx:298`, `BO/src/SecurityAccount.tsx:679`), top bar (`BO/src/Chrome.tsx:132,140`); default `title = "obop"` (`UI/src/logo.tsx:36`) | New `InstanceLogo` in `@obop/ui`: `full` is the artwork (an `<img>` of the logo route) plus the name as text, `mark` the artwork alone; no row means the existing `Logo`. The Brand book keeps `Logo`, since it documents obop's mark | S2 |
| BO-07 | Sign-in aside: "One business, one platform." and three bullets (`SignIn.tsx:188-199`) | obop's product copy | With a row the aside holds the logo and the name; the slogan and bullets are not shown | S2 |
| new | Sign-in message | none | `message_<lang>` under the heading of the back-office sign-in (all widths) and under the lead of the OAuth login page | S1, S2 |
| BO-30 | Recovery-codes download | `a.download = "obop-recovery-codes.txt"` (`UI/src/twofactor.tsx:402`), pinned by `web/packages/e2e/tests/mfa.spec.ts:94`; the component is shared with the control-plane app (`web/apps/platform/src/App.tsx:362`) | `<slug>-recovery-codes.txt` from the injected subset; the control-plane app has no brand and keeps `obop` | S2 |
| BO-11 | Brand book and Guide | Open to every signed-in person: the tab filter (`BO/src/Customize.tsx:70-77`), `personalCustomize` (`BO/src/App.tsx:197-200`), the account menu and palette entries (`BO/src/Chrome.tsx:161,309-314`), the Theme-tab notice (`Customize.tsx:396-405`), hash routes, and admin links from the Modules page (`BO/src/modules/ModulesPage.tsx:63`) and the Build tab (`BO/src/ModuleBuilder.tsx:283`) | With `hide_product_docs` one predicate `productDocsVisible()` gates all nine; the routes redirect to the first visible tab | S2 |
| BO-19 | Sign-link dialog: "obop sends no email, so give the link to the signer yourself" (`BO/src/SignLinks.tsx:269`) | names obop | Neutral reword for a row: "No email is sent, so …"; 7 locale catalogues | S2 |
| new | Customize tab "Identity" (admin only) | none | Form for the record over the three commands, with a preview of the sign-in card and the tile | S2 |
| OT-07 | MCP `initialize` | `serverInfo {name "obop", title "obop"}` and `INSTRUCTIONS` "This is an obop instance: an open-source business suite …" (`K/obop-mcp/src/lib.rs:21-27,254-257`; a second `serverInfo` at `K/obop-http/src/mcp.rs:850`) | `title` ← `name`, `name` ← `slug`; `INSTRUCTIONS` becomes a function: "This is the {name} system: business data …" without "open-source". Both `serverInfo` literals come from one function | S3 |
| OT-09, OT-10 | Discovery documents | `resource_name`, `title`, `description` say "obop instance"; `resource_documentation` and `service_documentation` are `https://obop.dev/docs/agents/mcp/` (`mcp.rs:754,756,822-825`, `K/obop-oauth/src/lib.rs:232`) | The three human fields take `name`; the two documentation links, optional in RFC 9728 and 8414, are left out. The identifiers `name` of the server card (`<host>/obop`), `dev.obop/auth`, `urn:air:obop:…` and `obop_client_classes_supported` stay (section 11) | S3 |
| OT-12 | OpenAPI `info.title` | "obop instance API" (`K/obop-http/src/openapi.rs:486`) | `{name} API`. The control-plane API title (`openapi.rs:127`) is SpaceCorps' own and stays | S3 |
| OT-16 | Slack messages | `obop: {event} ({model})` (`K/obop-connectors/src/runtime.rs:532`) | `{name}: …` | S3 |
| PA-01..15 | Phone app | Name under the icon, both usage prompts, five sign-in strings, client name and scheme all say obop (section 12, S4) | The build's identity block, not the server record (S4) | S4 |

## 7. Sentences in seven languages

The five catalogue keys of the OAuth pages put the product name inside a sentence, and the German, Finnish and Swedish cells attach it to a compound or a case ending (`obop-Instanz`, `obop-instanssiin`, `obop-tililläsi`, `obop-instansen`, counted from the catalogue). A bare placeholder is wrong in those languages and fits the other four only by luck. So a name is never inserted into a sentence: no sentence takes the name, and the five that name obop get neutral wording. The five keys get twins, `login.lead.neutral`, `consent.who_body.neutral`, `consent.f2.neutral`, `consent.who_body_mobile.neutral` and `consent.who_body_mobile_loopback.neutral`, in the seven languages, and the page uses them whenever a row exists, in every language. The 35 existing cells stay byte for byte for an instance with no row.

English drafts of the neutral twins (the placeholders `{app}`, `{host}` are the existing ones):

- `login.lead`: "{app} wants to connect to this instance. Sign in with your account to decide what it may do."
- `consent.who_body`: "{app} registered itself with this instance. This instance cannot verify who runs it. After you answer, your browser goes to {host}. Only continue if you started this connection yourself."
- `consent.f2`: "Steps that need approval still wait for a person to approve them in the back office. The application cannot approve its own proposals."
- `consent.who_body_mobile` and `…_loopback`: the existing sentences with "This instance cannot verify who made it." in place of "obop cannot verify who made it."

The six translations of each twin are machine-drafted, as the existing catalogue is (`K/obop-oauth/src/page.rs:420-422`; docs/ISSUES.md D7 and, for the signer page, O2), and are read by a native speaker or by counsel before a real customer's users see them (Q71 asks). The security meaning is kept word for word in every language: who runs it, the host the browser or phone goes to, and "only continue if you started this yourself". The name appears only in `.brand` (section 6), in the tab title of the back office and in the title of a sign-in screen, where it is a label and not part of a sentence.

Back-office strings are not touched except BO-19 (a single sentence in the seven locale catalogues). Admin-only strings keep naming obop on purpose (section 11).

## 8. The OAuth and signing pages: policy, logo, SVG, escaping

**Policy stays.** Both pages send `default-src 'none'; style-src 'sha256-<hash of CSS>'; …` and no `img-src` (`page.rs:151-165`, `K/obop-http/src/sign_page.rs:54-58`); the hash is computed from the fixed `CSS` constant, and the passkey script is allowed by its own hash. Rules this proposal keeps: no new script, no `style=` attribute, no new rule in `CSS` (a changed `CSS` changes the hash and every page's bytes), no `<img>`. So the logo on these two pages is inline `<svg>` with presentation attributes (`width`, `height`, `viewBox`, `fill`, `d`), which the policy does not govern. The one addition is the icon link: when it is emitted the two policies gain `img-src 'self'`, because browsers differ in whether a tab icon is held to `img-src` and which do is not verified here. With no row neither the link nor the directive exists.

**Serving the logo to an unauthenticated page.** There is no file serving. The artwork is one stored value, served by one fixed route (section 4) or inlined by the server into the page it renders. The server never serves an uploaded file as it came, never follows a URL in the artwork, and never takes a path or id.

**SVG rules.** Input is parsed by a strict, hand-written reader in `obop-brand`; the stored text is a re-serialisation from the parsed values, so a parser bug can refuse or mis-read a logo but cannot store hostile markup. Accepted: an optional `<?xml version … ?>` line; one root `<svg>` with a `viewBox` of four finite numbers (width and height are ignored, not stored); children `<path d>`, `<rect x y width height rx>`, `<circle cx cy r>` only, at most 64, nested nowhere; per element `fill` as `#rgb`, `#rrggbb`, `none` or `currentColor`, and `fill-rule`. The `d` of a path only holds the path commands, numbers and separators, at most 8 KiB. Refused with a named reason: `<script>`, `<style>`, `<foreignObject>`, `<image>`, `<use>`, `<text>`, `<a>`, gradients, filters, masks, animation, `<defs>`, a DOCTYPE, an entity or `&`, CDATA, a comment, a processing instruction other than the first line, any attribute not in the list above (`style`, `class`, `id`, `on*`, `href`, `xlink:*`, `xmlns:*`), any other `xmlns` than the SVG namespace, any URL or `url(`, input over 16 KiB, a second root. `currentColor` takes the text colour on the server-rendered pages and is black in an `<img>` (the back office); a logo for both light and dark uses explicit colours.

**Escaping.** `name`, titles and messages are written to HTML text with the existing `esc` (`page.rs:85-93`, and the signer page's copy), to attributes with `shell::attribute`, to JSON with `serde_json`, to the tab title with `esc`. The file name uses only the slug. The generated tile puts one validated letter into `<text>`, escaped. The reader re-validates every field (section 4), so a row written around the command cannot reach a page raw.

## 9. Caching

- Server-rendered pages already send `no-store` (`K/obop-http/src/oauth.rs:81`, `sign.rs:428`, `connectors.rs:128`); a change shows on the next render. There is no process-wide copy, so no stale brand across server processes.
- The shell is `no-cache`; the meta tag, title and icon link are made per request.
- The logo route is `no-cache` with an `ETag`, and its URL carries `?v=<logo_hash>`, so a new logo is a new URL. Browsers hold tab icons for a long time regardless; a changed logo may show in an old tab icon until the browser drops it (section 14).
- The cost is one `get` of one small row per page render, per shell request and per logo request.

## 10. Anti-phishing: what a brand setting cannot change

The consent page tells the person that the application registered itself and that the instance cannot verify who runs or made it, and shows where the result goes (`{host}`, or the phone scheme). That is the protection against a hostile client. The record may change how the page names itself, not what it says about the client. Fixed, whatever the row says:

1. The "Who is asking" box, its heading and its sentences are never read from the record. The neutral twins keep the claim ("This instance cannot verify who made it"); no field can shorten, hide or reword it. A test fails if a consent page with a row lacks the sentence, in any language (section 13).
2. The client's name is shown as it registered, escaped, in `<strong>`, and the brand name is never put in the sentence that names the client or on a button. A client that registers a name equal to the brand looks no more trusted than before; it could register "obop" today. The host or scheme line and the "cannot verify" sentence are the defence and stay.
3. The Microsoft and passkey buttons, the names of third parties (Microsoft, Google) and the connector provider names are never replaced by the brand.
4. Only a person who holds `platform.admin` can write the record; an agent cannot propose it; every change is in the security log with the old and new name.
5. The name and titles accept letters, digits and a short list of punctuation; no control, bidirectional or zero-width character, no markup, no URL or address form. Messages are plain text with no link. The logo cannot contain script, text or a reference to another resource.
6. What a message may say is the operator's business. It is shown on the instance's own pages and cannot take a visitor anywhere; the form tells the administrator that the text is readable by anyone.

## 11. Not covered on purpose

- **Wire names**: cookies (`obop_lang`, `obop_oauth_*`), the header `X-Obop-Applied`, `OBOP_*` variables, `obop.toml`, the CLI name `obop`, `@obop/*` packages, `dev.obop/auth`, `urn:air:obop:…`, `obop_client_class(es)`, `x-obop-*` keys, token prefixes `obop_`, `obopmfa_`, `obopsso_`, the MCP tool `obop__meta` (`K/obop-mcp/src/schema.rs:12`, named in the instructions), storage keys and the console prefix of the back office. Renaming the tool or the token prefix is a wire change for every client and prompt (Q71 asks; default keep).
- **Licence and attribution**: `LICENSE`, `NOTICE`, and any "Powered by obop" line, as section 1 says. Since #64 the image copies both files to `/usr/share/doc/obop/` (`Dockerfile`, the `COPY --chmod=0644 LICENSE NOTICE` line); this record does not touch them.
- **Operator and admin wording** that tells an administrator about running the software: update steps with the image name and the `obop` CLI, connector wizard texts, the module builder's `author`, module author/homepage/repository "SpaceCorps" (`K/obop-registry/src/lib.rs:720,723,724`), User-Agent strings, the SIE export header. They name the software the operator runs. They are admin-only and are the audit's slice S5, not part of this record.
- **Truthfulness items**, such as the Brand book and Guide still saying AGPL for bundled packages (the project is Apache-2.0), the sign-up app title and the host name prefix `obop-<slug>` that the control plane gives a provisioned instance (`K/obop-platform/src/sliplane.rs:251`): the audit's slice S6, separate small changes.
- The accent of the OAuth and signer pages (`#1d4ed8`, `page.rs:97`), which is not an obop colour; the forum and status pages of the control plane; the raster logo (PNG); a second artwork for the tab icon.
- The host name: `rp.id`, the signing link and the MCP URL follow `OBOP_PUBLIC_URL`.

## 12. Slices and sizes

Sizes use the audit's rule: S one literal, one asset or one existing setting in 1 to 2 files; M the same change in 3 or more files, all 7 locale catalogues, a value through two layers, or 3 or more pinned tests; L needs a new record or design, or 10 or more files. Slices S5 and S6 of the audit are outside this record (section 11).

| Slice | Contents | Audit rows | Size | Depends on |
|---|---|---|---|---|
| S1 | `obop-brand` (validation, slug, SVG reader, tile, neutral switch); model `instance_brand` with `get`, `configure`, `clear`, person-only guard, audit kinds, `obop admin brand`; `brand::current`; the OAuth pages, signer page and connector return page; neutral twins; `rp_name` in both builders; issuer fallback; public-site fallbacks; `/api/brand/logo.svg`; icon link and the two policy additions | SP-01..08, SP-11, OT-01, OT-03..06, BO-16 | L (new record and design, more than 10 files) | none |
| S2 | Shell rewrite (title, icon, `obop-brand` meta, `shell.rs` doc); `InstanceLogo` in `@obop/ui` and its use in the seven places; `document.title`; aside copy; `message`; recovery file name; `productDocsVisible()` in the nine places; BO-19 sentence; the Identity tab; 7 locale catalogues | BO-01, 02, 04..09, 11, 19, 30 (BO-12 and BO-14 are covered by BO-11) | M to L (about 13 rows; the Brand book hide alone is M, nine places) | S1 |
| S3 | `INSTRUCTIONS` as a function, one `serverInfo` function, discovery documents, OpenAPI title, Slack prefix. The tool `obop__meta` and the token prefix are Q71 | OT-07, 09, 10, 12, 16 (OT-08 asks) | M | S1 |
| S4 | The phone app. The strings under the icon, both usage prompts and the sign-in texts are fixed at build time (`Info.plist`, `strings.xml`, and a sign-in screen shown before the server is known), so they cannot come from the server row. They come from the build's identity block: `config/identity.ts` on draft PR 63 already holds `applicationId`, `scheme`, `clientName` and the four storage names in one place, and the host plan (`docs/spec/mobile-host.md` and its brand block, HP-01) adds the app name per locale, the two prompt texts with `{appName}`, icon, splash and notification text, and neutral or `{appName}` wording for the five sign-in strings. MOHV values: `MOHV`, `se.mohv.app`, `se.mohv.app.test`. The server's pages the phone opens in the system browser use the S1 record; the client name it registers is `identity.clientName` | PA-01..15, 16 (server lock), 18 (accent, open), HP-01, 02, 03, 07..09, 14 | M overall (30 rows; the brand block alone is L if designed here) | S1 for the browser leg; PR 63 and the host spec for the block |

Order: S1, then S2 and S3 in parallel, S4 alongside once PR 63 settles. An instance that sets nothing is unchanged after every slice.

## 13. Tests

**Every slice**

- *No row, same bytes.* The page bodies with no row are compared by SHA-256 with values captured from `main` before S1: the OAuth pages (sign-in, second factor, consent for an MCP client, for a phone, for a loopback client, error) in the seven languages; the seven states of the signer page in seven languages; both connector return pages; the shell (with and without `OBOP_DEFAULT_LANGUAGE`); the MCP `initialize` body; the registration options (`rp.name == "obop"`); the otpauth URI; the discovery and OpenAPI documents. The table changes only by a deliberate act.
- *With a row, no obop.* With name `MOHV` and an artwork, the visible text of every page above (tags removed, hidden inputs removed) contains no `obop` in any language, and the name and the logo are present.
- *Parity.* `instance_brand.get|configure|clear` exist as CLI, REST and MCP with the same schemas (the D80 gate).

**S1**

- `obop-brand`: a table of valid and refused names, titles, messages and slugs, including a bidirectional override, a zero-width space, `<b>`, `a@b.c`, `https://x`, over-length, and non-ASCII letters; slug derivation.
- SVG reader: the "M" artwork is accepted and re-serialises to the same text again (idempotent). A corpus of hostile inputs is refused with a named reason: script, `onload`, `<foreignObject>`, external `href`, `style`, `<use>`, an entity, DOCTYPE, CDATA, a comment, `javascript:` in any attribute, nested `<svg>`, a `url(` fill, oversized path, 65 elements, a second root. The output of the reader never contains a byte outside the allowed set (a property test over generated input).
- Policy: with no row the policy strings of both pages are today's; with a row (which always has a name, so always the icon link) they differ only by `; img-src 'self'`; the style hash is unchanged (the `CSS` constants are not edited).
- Escaping: a row written straight to the store with `<script>` in `name` and in a title renders escaped, or falls back to the default for that field.
- Commands: refused for an agent token, a scoped token, a mobile token and an approval proposal; allowed for a person with `platform.admin`; refused for a person without it; `get` open to any signed-in principal; two `security_event` rows; `clear` restores today's bytes.
- Passkey: `rp.name` is the name in the registration options on the REST and the OAuth builder; a passkey registered before the change still signs in after it.
- Issuer: precedence table (env set, record only, neither) for the label and for `issuer=`.
- Public site: each fallback and each explicit `site.*` key winning.
- Consent: with a row, the "Who is asking" sentence and the host or scheme line are present in all seven languages, for each of the three client classes.

**S2**

- Shell rewrite: title, icon link and meta present; escaping of a hostile `name`; a shell without `<title>` served as it is; `no-cache` kept; `304` and `206` handling as `shell.rs` tests it today.
- `web/packages/e2e` (Playwright over the built app): with a row, the sign-in screens at 1280 and 375 px have the name in the tab title and no `obop` in visible text or accessible names; the forced password change and second-factor set-up screens the same; the recovery-codes download is named `<slug>-recovery-codes.txt` (the pinned `mfa.spec.ts:94` keeps passing for no row); for an administrator and for a non-administrator the Customize tabs and the account menu show no Brand book and no Guide, the nine places are gone and the hash routes redirect; with no row they are as today.
- The Identity tab drives the three commands and nothing else.

**S3**: `initialize` with and without a row (`crates/obop/src/mcp_tests.rs:170` and `web/packages/e2e/tests/mcp.spec.ts:95` keep passing for no row); the tool list is unchanged; the documentation links are absent with a row.

**S4**: a `vp test` over the evaluated Expo config: with a brand block, no user-visible value (name, usage prompts, sign-in strings, client name, scheme) contains `obop`; `se.mohv.app` is the application id and the redirect scheme.

**The scan (the test that fails when a literal `obop` is added to a page that should use the record).** A Rust test `brand_scan_tests.rs` beside `addr_scan_tests.rs` and `audit_kind_tests.rs` in `K/obop-http/src`, on the reader of `src_scan.rs` (comments blanked, test-only code removed, literals kept apart from code). It reads the non-test code of a closed list of surface files (`obop-oauth/src/page.rs`, `obop-http/src/sign_page.rs`, `connectors.rs`, `passkeys.rs`, `oauth.rs`, `mcp.rs`, `openapi.rs`, `obop-mcp/src/lib.rs`, `obop-auth/src/mfa.rs`, `obop-connectors/src/runtime.rs`) and fails on any string literal that contains `obop`, case-insensitively, that is not a row of `ALLOWED`. A row names the file, the function, the literal and the count, and a class: `default` (the one place that spells the default name, `obop_brand::DEFAULT_NAME`), `wire`, `env`, `identifier`, `admin` or `today` (one of the 35 catalogue cells that stay for an instance with no row). A row that matches less than it says fails too. Most rows will be `wire` or `env`. Adding `"obop"` to `page.rs` fails with the file and line; adding a sixth catalogue key that names obop fails until it has a neutral twin. A second assertion reads the neutral twins and requires that none contains `obop`. A row in `ALLOWED` is the decision that a text may name obop on every instance and is reviewed against this document.

A second scan for the one reader: no code other than `brand::current` reads the model `instance_brand`.

A TypeScript counterpart, `brandLiterals.test.ts` with a reader in the style of `BO/src/i18nGuard.ts` (test support, not part of the app): over the non-test `.ts` and `.tsx` of `BO/src` and `UI/src` (the `brand/` directory and `Guide.tsx` excluded, since they document obop) and the values of `BO/locales/*/messages.json`, a string literal, JSX text or catalogue value that contains `obop` must be an `ALLOWED` row (storage keys, the console prefix, admin-only keys, the default name). `Logo` may be imported only by `InstanceLogo` and the `brand/` directory.

What the scans do not see: text built at run time, text in a dependency, a Rust or TypeScript file not on the list, a name inside a longer literal that is on the list for another reason, and text in a binary asset. They make a new literal a deliberate act; they do not stop a determined workaround.

## 14. Migration, rollout and honest limits

**Migration.** None for an instance that sets nothing. A new model adds rows only; records are JSON documents, so there is no DDL and no data step. Backups and restores carry the row as they carry any record; a restore of a backup made before the feature leaves the row absent. A binary without the feature has no code that reads the row. `CORE_VERSION` is not changed. `docs/DEPLOY.md` gets a section when S1 lands, and the line for `OBOP_MFA_ISSUER` there (`docs/DEPLOY.md:73`) states the precedence.

**Rollout for MOHV.** A deployment script runs `obop admin brand configure --name MOHV --hide-product-docs` once; swapping the logo later is the same command with `--logo-file`. The record is the only thing the image or the script needs to carry.

**Honest limits.**

- Nothing was built or run for this document. Page texts quoted from a render come from a separate check of `main`; the "with a row" column is design.
- That an inline SVG with presentation attributes passes a policy with `style-src` by hash follows from the policy text; it was not tried in a browser, and neither was `img-src` for a tab icon.
- Nothing was observed on a phone, and no operating-system passkey dialog was observed, so it is not known whether any platform shows `rp.name`, only the host, or both. `rp.name` is emitted only while adding a passkey. The statement that a client is not obliged to show it is from the WebAuthn text as remembered, not re-fetched.
- The six translations of every neutral twin are machine-drafted and unreviewed, like the catalogue they sit in.
- Tab icons are cached by browsers for a long time; a new logo may take a while to show. Authenticator entries already enrolled keep their old label.
- `hide_product_docs` hides entry points and redirects routes; the Brand book and the Guide stay in the JavaScript bundle (`BO/src/Guide.tsx:3` imports `docs/MODULES.md` as raw text). Taking them out of the bundle needs a build flag and is not sized here.
- One artwork serves as logo and tab icon; a wide wordmark makes a poor tab icon. In dark mode a logo with `currentColor` shows black in the back office.
- Which MCP clients show `serverInfo.title` and which `serverInfo.name` was not observed; that is why both follow the record.
- The tool name `obop__meta` and the token prefix still say obop; an end user who reads a tool list or a token sees them.
- `OBOP_DEFAULT_LANGUAGE` does not reach the OAuth pages, so a Swedish instance still shows an English OAuth page unless the client sends `lang` or `Accept-Language`. Unchanged here.
- The record is public by design. The Identity form says so.

## 15. Records and owner questions

Reserved, not entered here: decision **D94** (this design, PROPOSED), with the register row and record written with S1, as `docs/spec/mobile-host.md` does for D93: the D lines are the ones several open changes edit, and a record written before the design is reviewed would only add a conflict. D93, OR-39 and Q63 are reserved by `docs/spec/mobile-host.md`; Q62 is reserved for D87. **Q71** is entered with this document in `docs/DECISIONS.md` section 8: it is numbered after the highest question on any branch (Q70, on `main` since #62; no branch or pull request refers to Q71 or D94), and the counts of the identifier-namespace table (DECISIONS section 4 and the section 8 introduction) and the "Decision ids" row of CONVENTIONS 2.1 now say Q64 to Q71.

Q71 asks the owner, with these defaults (all PROPOSED):

1. Is the design right: one record, a person-only command, section 5's precedence, the generated tile as the placeholder logo? Default yes.
2. May the five OAuth sentence families use neutral wording, with no product name, for every instance that has a row (and keep today's text for one that has none)? Who confirms the six machine-drafted translations? Default yes; the owner with counsel, as for Q67.
3. May the MCP `serverInfo.name` follow the slug, and should the tool `obop__meta` stay (default keep; a rename is a wire change for every client and prompt)?
4. Does the token prefix `obop_` stay (default keep)?
5. May `hide_product_docs` be a UI hide only, with the content left in the bundle (default yes; a build flag later if wanted)?
6. Does the default violet accent before sign-in follow the instance theme (default yes), and is the accent otherwise out of scope?
7. Do the admin-only screens keep naming the software they run (default yes)?
