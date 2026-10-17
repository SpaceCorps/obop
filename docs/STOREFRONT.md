# The public website and web shop

- **Status:** working draft, 2026-10-17. Written against `crates/kernel/obop-site`, `crates/kernel/obop-payments`, `crates/kernel/obop-media`, `crates/kernel/obop-http/src/public.rs`, `crates/kernel/obop-commands/src/public.rs` and the packages `website` 0.4.0 and `ecommerce` 0.5.0. Decision records: D50 (the public site), D60 (images and files), D61 (stock, discount codes, taxes) and D62 (payments) in `docs/DECISIONS.md`.

Every instance can serve a **public website and web shop** to ordinary visitors: no login, no client framework, server-rendered HTML built only from what the installed `website` and `ecommerce` packages declare public and what is published. It has SEO and structured data, seven languages, branding from the instance settings and a strict safety model.

## Layout in the code

| Piece | Where |
|---|---|
| Declaration (`[model.public]`), gate | `crates/kernel/obop-pkg/src/manifest.rs`, `crates/kernel/obop-commands/src/public.rs` (`PublicSpec::check`) |
| The anonymous door: reads, submissions, receipts, rate limits | `obop_commands::PublicApi` |
| Rendering, routes, SEO, i18n, theme, seed (no web framework) | `crates/kernel/obop-site` (`Site::handle(&Req) -> Resp`) |
| Images and files: storage, validation, thumbnails | `crates/kernel/obop-storage/src/blob.rs`, `crates/kernel/obop-media`, `crates/kernel/obop-commands/src/blobs.rs` |
| Payments: providers, webhook, refunds, release of stock | `crates/kernel/obop-payments`, the port in `crates/kernel/obop-commands/src/pay.rs`, the door in `crates/kernel/obop-http/src/payments.rs` |
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
| `/cart`, `POST /shop/checkout`, `/shop/order/{token}` | cart (discount code, country, totals), order creation, confirmation |
| `/shop/pay/{token}`, `/shop/pay/{token}/continue`, `/shop/order/{token}/receipt` | where a provider sends the buyer back (the page asks the server, never the browser), the way back to an open payment page, the printable receipt |
| `POST /shop/discount/check`, `/cart?part=totals` | JSON check of a code, and the totals fragment the script fetches when the delivery method or the country changes |
| `/shop/fake-pay/{session}` | the demo provider's hosted page (only with `OBOP_DEMO_PAYMENTS=1`) |
| `/media/{id}.{png,jpg}[?w=320\|640\|960\|1280]` | a public picture: only blobs held by a published record through a declared public field |
| `POST /hooks/pay/{provider}` | the payment webhook (served on every instance, not only with the public site) |
| `/contact` | form (creates `website_form_submission`); a published page with slug `contact` supplies the intro text |
| `/sitemap.xml`, `/feed.xml`, `/robots.txt`, `/llms.txt`, `/.well-known/security.txt`, `/favicon.svg` | machine documents |
| `/assets/site.css`, `/assets/cart.js` | the one stylesheet (generated from the accent) and the one script, immutable with `?v=` |

Slugs `blog shop cart contact home admin api assets` and the machine documents are reserved for the site itself.

### Pictures

`image` and `file` fields hold the id of a stored blob (D60). The listing has a main `image` and `image_alt`, and any number of gallery pictures in `ecommerce_listing_image` (a listing, a picture, alt text, a position and its own `published` flag); pages and posts have `cover_image` and `cover_alt`. The shop renders real pictures only when `PublicApi::public_blob_info` says the blob is public (held by a published record through a field the package declared public, and for a gallery picture also with its listing published); otherwise the card keeps its monogram. Every `<img>` has a `srcset` over the fixed thumbnail widths (320, 640, 960, 1280, only those narrower than the original, plus the original when it is at most 1280 wide), `sizes`, `width` and `height` (no layout shift), the alt text (the field's, else the title) and `loading="lazy"`; the main product picture and a page's cover are eager with `fetchpriority="high"`. `og:image`, `og:image:width`, `og:image:height`, `og:image:alt`, `twitter:image` (and the card becomes `summary_large_image`), the `image` list of the product's JSON-LD and the sitemap image extension carry the same pictures. `/media/{id}.{png|jpg}` answers with the content type, `nosniff`, a strong ETag and `Cache-Control: public, max-age=31536000, immutable`; thumbnails are made on first request, cached, and never larger than the original.

### The cart

The cart is kept in the browser's `localStorage` (`obop-cart-v1`, ids and quantities only). The cart page is server-rendered from `/cart?items=ID.QTY~ID.QTY`: the script redirects there from the stored cart, so every price on the page comes from the server. Without JavaScript everything still works: "Add to cart" is a link to `/cart?add=ID`, the page lets you change quantities, remove lines, add another product and **apply a discount code** (`?code=`), and the URL is the cart. The totals block (subtotal, discount line, delivery, one line per tax, total) is rendered by the server; when the delivery method or the country changes, the script fetches only that block (`/cart?...&part=totals`) and swaps it in.

### Checkout

`POST /shop/checkout` accepts only listing ids and quantities, the buyer's name, email and address, a delivery method id, a country, how to pay, the code text, the CSRF token, an idempotency token and a hidden honeypot field. The server looks up price, name, currency and stock from the **published listing**, prices the delivery method from the published shipping method (free above its threshold, judged on the lines at listing prices), finds the discount code (D61), picks the tax rates and computes the totals with exact integers (`pricing.rs`: per-line tax, half away from zero, included or added on top). Quantities are 1 to 99, at most 50 lines, one currency per order. Then **one atomic batch** (`PublicApi::submit_order`) writes the `ecommerce_order` (state `pending`, `stock_returned` false, `reserved_until`), all its `ecommerce_order_line` records (with their discount share and tax), takes the stock of every tracked line (never below zero for `block_when_out`) and counts the code's use (against its maximum and the per-customer counter keyed by a hash of the email address). If any step does not hold, nothing is written and the buyer is told precisely what is left. The order number is random (`W20261016-K7Q2ZD`), so it does not reveal how many orders exist.

Payment is a separate, server-side step (D62). With `site.payment_methods` listing more than one way the buyer chooses; the choice is checked against what the instance offers (`stripe` needs a connected `stripe` connection, `fake` needs `OBOP_DEMO_PAYMENTS=1`). `manual` shows the instructions on the order page; `stripe` and `fake` create a payment record and a hosted page and redirect the buyer there. The buyer returns to `/shop/pay/{token}`: the page asks the provider and shows the server's state, so a forged return or `?s=...` changes nothing. The webhook marks the order `paid` once, cross-checking the amount and currency with the stored order. A payment that fails, expires or is cancelled cancels the order and puts the stock and the code use back, once; an unpaid order whose reservation ran out is cancelled the same way. Refunds are the order's `refund` action: the provider is asked first, and an order whose refund is refused stays paid. There is no email receipt (no mail sender); the confirmation page is the receipt, with a printable page.

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
| `site.payment_methods` | `manual` | comma separated, in order: `manual` (bank transfer or invoice), `stripe` (card, Stripe Checkout; offered only when the instance has a connected `stripe` connection), `fake` (demo; offered only with `OBOP_DEMO_PAYMENTS=1`) |
| `site.payment_instructions` | empty | shown to buyers who pay by transfer: account, reference, due date (1000 characters) |
| `site.default_country` | empty | two-letter country preselected at checkout when tax rates differ by country |
| `site.payment_hold_days` | `7` | days an unpaid transfer order holds its stock before it is cancelled (1 to 60); online payments hold it 45 minutes |

## Localisation

Chrome (navigation, buttons, forms, errors, dates, money) is in English, German, French, Polish, Finnish, Swedish and Spanish (`i18n.rs`; one table row per key with seven columns, a test enforces completeness and matching placeholders). The language is chosen by `?lang=`, then the `lang` cookie, then `Accept-Language` (q-values honoured), then `site.default_lang`. Money uses the listing's currency and the visitor's locale (`€1,234.50`, `1.234,50 €`, `1 234,50 €`, `1234,50 €` for four-digit Spanish, symbol placement, minor units of JPY and KWD); dates are written the way the language writes them.

Records carry translations through sibling models: `website_page_translation`, `website_blog_post_translation`, `ecommerce_listing_translation` (a ref to the record, a `lang`, the translated fields and a `published` flag). The renderer uses the translation for the visitor's language when it exists and is published, else the base text, marked with `lang=` on the content so screen readers pronounce it right. `hreflang` alternates (and `xhtml:link` in the sitemap) list only languages that exist, plus `x-default`; lists and the home page without a home page list all seven because their chrome exists in all.

## SEO and agent readiness

Every page: unique title and description (from the record, with fallbacks), canonical (in the language of the text), Open Graph and Twitter tags, JSON-LD (`WebSite` with a `SearchAction`, `Organization`, `Product` with `Offer` price, currency and availability `InStock/OutOfStock/BackOrder`, `BlogPosting`, `WebPage`, `BreadcrumbList`, `ItemList`), `rel=prev/next`, real status codes (`200 301 403 404 405 410 413 429 503`), `ETag` and `Last-Modified` with `304`, `Cache-Control: public, max-age=60, stale-while-revalidate=300` (anything with a cookie or a token is `private, no-store`), `Vary: Accept, Accept-Language, Cookie`. **Every page is also Markdown** with `Accept: text/markdown` or `?format=md`, rewritten from the parsed body so it never carries raw HTML. `llms.txt` lists the published pages, posts and products with `?format=md` links. The sitemap has `lastmod` (from `published_on` where a record has one) and language alternates; `robots.txt` allows crawlers and disallows `/admin/`, `/api/`, `/cart`, `/shop/checkout`, `/shop/order/`. HTML is semantic (landmarks, skip link, one `<h1>`, labelled fields with `aria-describedby`, `role=alert` error summary, visible focus, 44 px targets, `prefers-color-scheme` and `prefers-reduced-motion`), with no layout shift from fonts (system fonts) and aspect-ratio product cards.

## Packages

`website` 0.4.0: `website_page`, `website_blog_post` (public through `state = published`; archived answers 410), `website_menu_item` (public through `visible`), `website_form_submission` (public submit), the two translation models; `cover_image` and `cover_alt` on pages and posts. `ecommerce` 0.5.0: `ecommerce_category`, `ecommerce_listing` (with `image`, `image_alt`, `stock_policy`, `stock_quantity`; `server_adjust = ["stock_quantity"]`), `ecommerce_listing_image`, `ecommerce_listing_translation`, `ecommerce_shipping_method` public (`active`), `ecommerce_discount_code` and `ecommerce_tax_rate` (readable by the server only, `server_read`), `ecommerce_discount_use` (per-customer counters, a hash and a count), `ecommerce_order` and `ecommerce_order_line` (public submit and token receipt; the receipt now carries discount, tax lines and the payment method), `ecommerce_payment` (provider, reference, amount, currency, state, applied event ids; its state moves only through its provider and the order actions; part of the order's receipt as provider, state, amount, currency). There is no data upgrade: add the new fields by hand to existing records.

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
| Two buyers racing for the last unit; stock below zero | stock is taken in the order's atomic batch with a minimum of zero (`Backend::apply_checked`, row locks on PostgreSQL) | `security_two_buyers_racing_for_the_last_unit_get_one_order_and_stock_never_goes_negative`, `an_order_takes_its_units_off_the_listing_and_the_last_unit_goes_once` (obop-site `tests_shop`), `atomic_contract` (obop-storage, memory, SQLite and PostgreSQL) |
| Guessing discount codes; using a code past its maximum; learning which codes exist | one generic refusal for every reason, ten failed attempts per ten minutes per address, uses counted in the same atomic batch against `max_uses` and a per-customer counter | `security_every_reason_a_code_fails_gives_the_same_answer`, `security_failed_attempts_are_limited_per_address_and_the_check_endpoint_says_the_same_thing`, `security_a_code_cannot_be_used_past_its_maximum_even_when_buyers_race` |
| Tampering with discount, tax, total, state, stock, payment method, country, quantity, price | none of them is read from the browser; quantities 0, negative, fractional, huge and non-numeric are refused; sums that overflow are refused; a method that is not offered is refused | `security_nothing_the_browser_sends_sets_a_discount_a_tax_or_a_total`, `overflowing_sums_are_refused_not_wrapped` (pricing) |
| Tax or discount rounding that does not add up | one documented rule (per line, half away from zero; discount split by largest remainder), exact `i128` arithmetic | `pricing::tests` (rounding, allocation, included and added tax, rate choice, category discount) |
| Forged return from the provider, forged cancel, paying one order with another's session, cross-order tokens | the pay page ignores the query string and asks the provider; tokens are 256-bit and show only their order; a demo session belongs to one payment | `security_the_return_page_never_trusts_the_browser_and_tokens_belong_to_one_order` (obop-payments) |
| Forged, unsigned, stale, replayed or mismatched webhooks | `Stripe-Signature` checked on the raw body first (HMAC-SHA256, five minutes, constant time), replay record in the database and applied-event ids on the payment, amount and currency cross-checked, one answer (401) for every signature failure, 256 KiB cap, rate limit before signature work | `security_unsigned_forged_stale_and_mismatched_notifications_change_nothing`, `a_signed_completion_marks_the_order_paid_once_and_a_replay_changes_nothing`, `security_two_deliveries_of_one_event_racing_pay_the_order_once` (obop-payments), `the_webhook_door_checks_the_signature_before_anything_else`, `security_the_webhook_body_is_capped_and_only_post_is_taken` (obop) |
| The demo provider switched on by accident | needs `OBOP_DEMO_PAYMENTS=1` from the operator and `fake` in the owner's settings; otherwise not offered and its pages answer 404 | `security_the_demo_provider_is_off_unless_the_operator_turned_it_on` |
| Double refund, refund of money not received, a refund the provider refused | the provider is asked first with a fixed idempotency key; a refused refund leaves the order paid; a refunded payment is left alone; agents only propose | `a_refund_returns_the_money_through_stripe_records_the_reference_and_restocks`, `if_stripe_refuses_the_refund_the_order_stays_paid`, `a_delivered_order_is_refunded_without_putting_goods_back_on_the_shelf` |
| Money for a cancelled order; stock or code held forever | a payment arriving for a cancelled order is refunded; unpaid orders are cancelled when their reservation ends; release is idempotent (`stock_returned` in the same batch) | `a_payment_that_arrives_for_a_cancelled_order_is_returned_not_kept`, `unpaid_orders_whose_reservation_ran_out_are_cancelled_and_released`, `a_failed_or_abandoned_payment_gives_the_units_and_the_code_back`, `cancelling_an_unpaid_order_by_hand_puts_the_stock_back_once` |
| Stripe key leaking | read per call from the sealed connection, sent only as a bearer header, never in an error, a log or a request body; no customer data is sent to Stripe | `the_keys_are_never_printed`, `stripe_checkout_creates_a_session_from_the_server_total_and_nothing_personal` |
| Path injection through a provider reference | a session id must match `cs_[A-Za-z0-9_]+` before it enters a URL | `a_session_id_is_checked_before_it_becomes_part_of_a_url` |
| A private picture read by someone else, or by the public; a guessed blob id | reads need read access to a record that holds the blob (agents never get bytes); the public route needs a published holder through a declared public field; ids are 64 hex characters checked before any lookup | `reads_follow_the_permissions_of_the_owning_record_and_agents_get_no_bytes`, `a_blob_is_public_only_through_a_published_record_and_a_declared_field` (obop-commands), `security_unpublished_private_and_unattached_blobs_are_never_public` (obop-site), `upload_attach_download_and_thumbnail_over_http`, `ids_are_hashes_never_paths_and_other_peoples_uploads_are_not_usable` (obop) |
| Hostile uploads: SVG with script, polyglots, mislabelled files, decompression bombs, oversized bodies, EXIF GPS | content sniffing, SVG and markup refused, decoding limits before allocation, re-encoding, size cap by `Content-Length` and by bytes read, a rate limit per principal | `svg_is_always_rejected`, `trailing_scripts_are_not_kept_and_polyglots_are_refused`, `size_and_dimension_limits_hold_before_pixels_are_allocated`, `exif_gps_and_author_do_not_survive` (obop-media), `uploads_need_a_token_a_writable_image_field_and_honest_content`, `uploads_are_rate_limited_per_principal` (obop), e2e `uploads are checked` |
| Path traversal in blob ids and media URLs | the id is validated as 64 lowercase hex before a path, a key or a query is built; the directory store builds paths from validated keys only | `ids_are_content_hashes_and_malformed_ones_never_pass`, `memory_directory_and_sqlite_bytes_meet_the_contract` (obop-storage), `media_is_served_immutable_with_a_hash_etag_nosniff_and_fixed_thumbnail_widths` |
| A visitor uploading files, or a photo of a person being public | the gate refuses `image` and `file` in `submit_fields` and personal data in public fields | `the_gate_refuses_public_photos_of_people_and_visitor_uploads` |

Left open: see Known gaps (shared-address rate limits, in-process counters, caches that already hold a picture).

## Known gaps

- **Not tested against a real provider.** Stripe is exercised against a local stand-in that speaks the same shapes; the first live test needs the owner's account and is F2-style work.
- **No email receipts and no VAT engine.** There is no mail sender, so the confirmation page and a printable receipt are the receipt. VAT identification numbers, reverse charge, the One-Stop-Shop and destination thresholds are out of scope (D61); the owner enters the rates.
- **No object store.** Blob bytes live in a directory or the database; an S3-compatible store is one more `BlobBytes` implementation (D60). No antivirus unless an operator installs a `Scanner`.
- **Pictures are re-encoded.** JPEG quality, ICC profiles and animation are not kept. A CDN or browser that cached a public picture keeps it after the record is unpublished until it is purged.
- **Free delivery is judged on the lines before the discount.**
- **Rate limits and idempotency keys are in the database** (D59): shared by every process and surviving a restart, as fixed windows (a burst at a window edge can pass twice the limit); behind NAT many visitors share one address (a failed discount attempt is held against the address). The upload limiter is in memory, per process.
- **Reads scan the model's records** (D49 limits apply): fine for hundreds of listings, not for hundreds of thousands. Put a CDN in front: pages are cacheable.
- **Feeds and `llms.txt` are in the default language;** `lastmod` exists only for records with `published_on`.
- A refund holds the dispatcher's transaction lock while it calls the provider (up to the 20 second client timeout).
- An agent with `ecommerce.manager` can set a listing's `published` flag directly (the boolean has no approval); pages and posts are protected through `publish`.
