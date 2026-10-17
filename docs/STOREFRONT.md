# The public website and web shop

- **Status:** working draft, 2026-10-16. Written against `crates/kernel/obop-site`, `crates/kernel/obop-http/src/public.rs`, `crates/kernel/obop-commands/src/public.rs` and the packages `website` and `ecommerce` 0.2.0. Decision record: D50 in `docs/DECISIONS.md`.

Every instance can serve a **public website and web shop** to ordinary visitors: no login, no client framework, server-rendered HTML built only from what the installed `website` and `ecommerce` packages declare public and what is published. It has SEO and structured data, seven languages, branding from the instance settings and a strict safety model.

## Layout in the code

| Piece | Where |
|---|---|
| Declaration (`[model.public]`), gate | `crates/kernel/obop-pkg/src/manifest.rs`, `crates/kernel/obop-commands/src/public.rs` (`PublicSpec::check`) |
| The anonymous door: reads, submissions, receipts, rate limits | `obop_commands::PublicApi` |
| Rendering, routes, SEO, i18n, theme, seed (no web framework) | `crates/kernel/obop-site` (`Site::handle(&Req) -> Resp`) |
| axum adapter and serving layout | `crates/kernel/obop-http/src/public.rs` |
| CLI | `crates/obop/src/main.rs` (`serve --public-site`, `site seed|set|show`) |

## Build and run

```bash
# 1. the backoffice, built for /admin/
cd web/apps/backoffice && OBOP_ADMIN_BASE=/admin/ vp build && cd ../../..

# 2. an instance with the two packages and demo content
cargo build --release
obop modules install-from website --db sqlite://shop.db
obop modules install-from ecommerce --db sqlite://shop.db
obop site seed --db sqlite://shop.db            # invented tea and ceramics shop, blog, translations
echo "a long password" | obop admin create owner --all-groups --db sqlite://shop.db

# 3. serve: public site at /, backoffice at /admin/, API at /api
obop serve --public-site --static web/apps/backoffice/dist --db sqlite://shop.db --addr 127.0.0.1:8080
```

- `--public-site` or `OBOP_PUBLIC_SITE=1` turns the public site on. Without it the instance behaves exactly as before: the backoffice is at `/`.
- `--admin-prefix /admin` (or `OBOP_ADMIN_PREFIX`) is one path segment; unknown paths below it fall back to the backoffice `index.html`.
- `--trust-proxy` (or `OBOP_TRUST_PROXY=1`) takes the client address for rate limits from the last entry of `X-Forwarded-For`. Set it only behind a proxy you control.
- `OBOP_PUBLIC_URL=https://shop.example.com` (or the setting `site.base_url`) is the origin used in canonical links, the sitemap and feeds. Set it in production; otherwise the request `Host` is used (sanitised).
- A Docker image does not change: build the backoffice with `OBOP_ADMIN_BASE=/admin/`, pass `--public-site` to `obop serve`.

## Routes

| URL | Content |
|---|---|
| `/` | the published page with slug `home`, else a generated shop home (hero, featured products, latest posts), else a friendly "not published yet" page with a link to `/admin/` |
| `/{slug}` | a published `website_page`; archived pages answer `410`, drafts and unknown slugs the same `404` |
| `/blog`, `/blog/{slug}` | published posts, newest first, 10 per page |
| `/shop`, `/shop/{id}-{slug}` | product grid with `?cat=`, `?q=` and `?page=`; product pages (a wrong slug is a `301` to the canonical URL) |
| `/cart`, `POST /shop/checkout`, `/shop/order/{token}` | cart, order creation, confirmation |
| `/contact` | form (creates `website_form_submission`); a published page with slug `contact` supplies the intro text |
| `/sitemap.xml`, `/feed.xml`, `/robots.txt`, `/llms.txt`, `/.well-known/security.txt`, `/favicon.svg` | machine documents |
| `/assets/site.css`, `/assets/cart.js` | the one stylesheet (generated from the accent) and the one script, immutable with `?v=` |

Slugs `blog shop cart contact home admin api assets` and the machine documents are reserved for the site itself.

### The cart

The cart is kept in the browser's `localStorage` (`obop-cart-v1`, ids and quantities only). The cart page is server-rendered from `/cart?items=ID.QTY~ID.QTY`: the script redirects there from the stored cart, so every price on the page comes from the server. Without JavaScript everything still works: "Add to cart" is a link to `/cart?add=ID`, the page lets you change quantities, remove lines and add another product, and the URL is the cart.

### Checkout

`POST /shop/checkout` accepts only listing ids and quantities, the buyer's name, email and address, a delivery method id, the CSRF token, an idempotency token and a hidden honeypot field. The server looks up price, name, currency and stock from the **published listing**, prices the delivery method from the published shipping method (free above its threshold), checks quantities (1 to 99, at most 50 lines, at most the stock for `block_when_out`, one currency per order) and creates the `ecommerce_order` in state `pending` with all its `ecommerce_order_line` records in **one transaction**. The buyer is redirected to `/shop/order/{token}` (256-bit token), which shows only what that order contains. The order number is random (`W20261016-K7Q2ZD`), so it does not reveal how many orders exist.

**There is no payment provider.** The page says "we will contact you with payment details". To plug one in: add the provider's API call and webhook as a server-side step after `submit_batch` in `Site::checkout` (redirect to the provider with the order total from the stored order), and have the webhook run the `ecommerce_order.mark_paid` action as a service principal. `payment_reference` (class `financial`) already exists on the order.

## Settings

The owner's branding lives in the instance settings object (the same JSON the Customize app stores, under the key `site`). Set a key with `obop site set KEY VALUE --db URL`, list them with `obop site show`, or write `{"site": {...}}` through the admin command `setting.update`. Values are validated on every read: an invalid value falls back to the default, nothing the owner types can break out of an attribute, a URL or the stylesheet. Settings are cached for 5 seconds.

| Key | Default | Meaning |
|---|---|---|
| `site.enabled` | `true` | `false` answers every page with `503` and `Retry-After`; `robots.txt` disallows everything |
| `site.title` | `My Shop` | site name (80 characters), used in titles, JSON-LD, the footer |
| `site.tagline` | empty | one line (160), the home page title suffix and meta description fallback |
| `site.logo_text` | the title | text logo (40) |
| `site.logo_url` | none | `https://...` or `/path` image shown before the text; its https origin is added to the CSP `img-src` |
| `site.accent` | `#0f766e` | brand colour; adjusted per light and dark mode until button text and accent text meet WCAG AA (same algorithm as the backoffice colour picker, `theme.rs`) |
| `site.footer_text` | empty | footer sentence (300) |
| `site.social` | none | JSON object label to https URL, at most 8; becomes links and `Organization.sameAs` |
| `site.default_lang` | `en` | language of the content and default for visitors: `en de fr pl fi sv es` |
| `site.base_url` | none | public origin without path; wins over `OBOP_PUBLIC_URL` and the `Host` header |
| `site.noindex` | `false` | staging mode: `noindex` meta and header, `Disallow: /` |
| `site.security_contact` | none | email or https URL published in `/.well-known/security.txt` (404 without it) |
| `site.shop_currency` | `EUR` | used when a listing names no currency |
| `site.shop_page_size` | `12` | products per page, 4 to 48 |
| `site.checkout_note` | empty | extra sentence under the checkout button |

## Localisation

Chrome (navigation, buttons, forms, errors, dates, money) is in English, German, French, Polish, Finnish, Swedish and Spanish (`i18n.rs`; one table row per key with seven columns, a test enforces completeness and matching placeholders). The language is chosen by `?lang=`, then the `lang` cookie, then `Accept-Language` (q-values honoured), then `site.default_lang`. Money uses the listing's currency and the visitor's locale (`€1,234.50`, `1.234,50 €`, `1 234,50 €`, `1234,50 €` for four-digit Spanish, symbol placement, minor units of JPY and KWD); dates are written the way the language writes them.

Records carry translations through sibling models: `website_page_translation`, `website_blog_post_translation`, `ecommerce_listing_translation` (a ref to the record, a `lang`, the translated fields and a `published` flag). The renderer uses the translation for the visitor's language when it exists and is published, else the base text, marked with `lang=` on the content so screen readers pronounce it right. `hreflang` alternates (and `xhtml:link` in the sitemap) list only languages that exist, plus `x-default`; lists and the home page without a home page list all seven because their chrome exists in all.

## SEO and agent readiness

Every page: unique title and description (from the record, with fallbacks), canonical (in the language of the text), Open Graph and Twitter tags, JSON-LD (`WebSite` with a `SearchAction`, `Organization`, `Product` with `Offer` price, currency and availability `InStock/OutOfStock/BackOrder`, `BlogPosting`, `WebPage`, `BreadcrumbList`, `ItemList`), `rel=prev/next`, real status codes (`200 301 403 404 405 410 413 429 503`), `ETag` and `Last-Modified` with `304`, `Cache-Control: public, max-age=60, stale-while-revalidate=300` (anything with a cookie or a token is `private, no-store`), `Vary: Accept, Accept-Language, Cookie`. **Every page is also Markdown** with `Accept: text/markdown` or `?format=md`, rewritten from the parsed body so it never carries raw HTML. `llms.txt` lists the published pages, posts and products with `?format=md` links. The sitemap has `lastmod` (from `published_on` where a record has one) and language alternates; `robots.txt` allows crawlers and disallows `/admin/`, `/api/`, `/cart`, `/shop/checkout`, `/shop/order/`. HTML is semantic (landmarks, skip link, one `<h1>`, labelled fields with `aria-describedby`, `role=alert` error summary, visible focus, 44 px targets, `prefers-color-scheme` and `prefers-reduced-motion`), with no layout shift from fonts (system fonts) and aspect-ratio product cards.

## Packages (0.2.0)

`website`: `website_page`, `website_blog_post` (public through `state = published`; archived answers 410), `website_menu_item` (public through `visible`), `website_form_submission` (public submit), the two translation models; new field `byline` on posts (the private `author_name` stays internal). `ecommerce`: new `ecommerce_category`, `ecommerce_listing_translation`; listings gain `summary`, `category_id`, `currency`, `sku`, `stock_quantity`; `ecommerce_shipping_method` public (`active`); `ecommerce_order` and `ecommerce_order_line` public submit and token receipt (`access_token`, class `secret`, never readable); order lines gain `listing_id` and `product_id` is now optional. There is no data upgrade: add the new fields by hand to existing records.

## Threat model

Anonymous access reaches only what a package declares. Each finding has a named test (`crates/kernel/obop-site/src/tests.rs` unless noted).

| Threat | Mitigation | Test |
|---|---|---|
| Reading fields that were not meant to be public | whitelist per model, gate refuses personal data, non-`none` classes and restricted fields; a ref is embedded only if its target is public and published | `only_published_records_and_only_whitelisted_fields_are_public`, `the_gate_refuses_personal_restricted_and_unknown_fields`, `a_ref_is_embedded_only_when_the_target_is_public_and_published` (obop-commands), `security_private_fields_of_published_records_are_never_rendered` |
| Enumerating unpublished records | unpublished and nonexistent give the identical response; 410 only for a `gone_value` state that was public before | `security_unpublished_and_nonexistent_products_are_indistinguishable`, `unknown_unpublished_and_archived_pages_use_real_status_codes` |
| A bearer token or agent acting as "the public" | no principal: `PublicApi` bypasses `Dispatcher::call`; the anonymous principal has no groups and an empty scope | `the_anonymous_principal_satisfies_no_group` (obop-commands) |
| Personal data leaking | submissions and order details appear on no public page; only the token holder sees an order | `security_nothing_a_visitor_submitted_appears_on_any_public_page`, `security_the_confirmation_page_is_reachable_only_with_the_token_and_shows_only_that_order` |
| Order tampering (price, total, state, currency, token sent by the browser) | prices, names, currency, delivery and totals computed from published records; visitor keys limited to `submit_fields`, server fields cannot be overridden | `security_checkout_never_trusts_prices_totals_or_state_from_the_browser`, `server_fields_cannot_be_overridden_by_the_visitor` (obop-commands) |
| Ordering unpublished listings or foreign delivery methods | refs a visitor supplies must be published records of public models | `security_checkout_refuses_unpublished_and_unknown_listings`, `a_visitor_cannot_reference_an_unpublished_record` (obop-commands) |
| Mass assignment | `submit_fields`, unknown keys refused, governed state starts at its first option | `a_submission_takes_only_declared_fields_and_enforces_the_model` (obop-commands), `security_mass_assignment_on_the_contact_form_is_ignored` |
| Invalid quantities, stock, currency mixes, overflow | whole numbers 1 to 99, stock checked, one currency, checked arithmetic and a total cap | `security_checkout_validates_quantities_stock_and_fields`, `a_listing_with_a_mismatched_currency_cannot_be_ordered_with_others` |
| CSRF | double-submit cookie (`obop_csrf`, constant-time compare), `SameSite=Lax`, and an `Origin` that is present must match `Host` | `security_checkout_needs_a_csrf_token_that_matches_the_cookie` |
| Bots, floods, double submits | honeypot field, per-address burst limit, the package's per-source and global hourly limits, 32 KiB body cap, idempotency key | `security_the_honeypot_creates_nothing_and_looks_like_success`, `security_checkout_is_rate_limited_per_address_and_bodies_are_capped`, `security_the_model_rate_limit_stops_a_flood_of_orders`, `security_contact_submissions_are_rate_limited_by_the_package`, `security_a_double_submit_creates_one_order` |
| Token guessing | 256-bit random token, constant-time comparison, per-address limit on lookups, short tokens refused | `security_the_confirmation_page_is_reachable_only_with_the_token_and_shows_only_that_order` |
| XSS in titles, bodies, names, settings | every dynamic value escaped; bodies pass `pulldown-cmark` then `ammonia` (no raw HTML, no images, schemes `http https mailto`, no `//` links); JSON-LD escapes `< > &`; no inline script or style, CSP `default-src 'none'` | `security_everything_owner_written_is_escaped_in_every_place_it_is_used`, `security_json_ld_cannot_close_its_script_tag`, `security_dangerous_links_and_html_in_markdown_bodies_are_neutralised`, `the_contact_form_validates_and_keeps_what_the_visitor_typed_escaped`, and the `md.rs` tests |
| Clickjacking, sniffing, referrer leaks | `X-Frame-Options: DENY`, `frame-ancestors 'none'`, `nosniff`, HSTS over https, `no-referrer` on the order page | `security_every_response_carries_the_clickjacking_and_content_policy_headers` |
| Header injection, open redirects | `lang` values are parsed against a list; redirect targets come from stored slugs and fixed paths; `?next=` does not exist | `language_comes_from_query_then_cookie_then_header_then_the_default`, `a_wrong_product_slug_redirects_to_the_canonical_url_and_nothing_else` |
| Cache poisoning | `Vary: Accept, Accept-Language, Cookie`; pages that set a cookie are `no-store`; with `site.base_url` set the `Host` header cannot choose canonical URLs; a hostile `Host` is never echoed | `security_caches_are_told_what_the_response_depends_on` |
| SSRF | none: the crate has no HTTP client and no filesystem or process access | `security_the_site_makes_no_outbound_requests_and_logs_nothing` |
| Personal data in logs | the crate does not log; rate-limit keys are hashes of the address | same test |
| Declaration mistakes | the gate runs at parse, in `to_module` and again at registration | `crates/kernel/obop-pkg/src/public_tests.rs` |

Left open: see Known gaps (shared-address rate limits, in-process counters, no stock reservation).

## Known gaps

- **No payments.** Orders start `pending`; see "Checkout" for where a provider plugs in.
- **No images.** The data model has no file field. Product and post cards are a text monogram on a tint of the accent; Markdown images become links; logos are a setting. Open Graph has no image.
- **Stock is not reduced** by orders, and discount codes are not accepted at checkout (codes would be an oracle for guessing). Taxes are included in prices; `tax_total` is not computed.
- **Rate limits and idempotency keys are in the database** (D59): shared by every process and surviving a restart, as fixed windows (a burst at a window edge can pass twice the limit); behind NAT many visitors share one address.
- **Reads scan the model's records** (D49 limits apply): fine for hundreds of listings, not for hundreds of thousands. Put a CDN in front: pages are cacheable.
- **Feeds and `llms.txt` are in the default language;** `lastmod` exists only for records with `published_on`.
- The checkout cart total shown before the last step is for the selected delivery method (a script updates it when you change the method; without the script the preselected one is shown); the confirmation page shows the real figures.
- An agent with `ecommerce.manager` can set a listing's `published` flag directly (the boolean has no approval); pages and posts are protected through `publish`.
