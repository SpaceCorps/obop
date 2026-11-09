// What the server tells the page about the instance's brand before anyone has signed in
// (docs/spec/branding.md, section 4): `<meta name="obop-brand" content='{"name":…}'>`. The shell
// adds it only when a brand row is set, and puts nothing else in it than the name, the effective
// slug, the messages that are set, the version of the logo and, when the theme stores one, the
// accent. No titles and no switch: the switch `hide_product_docs` is read after sign-in.
//
// A page without the tag, or with one that is not what the server writes, has no brand: every
// reader here then answers as before the feature (`null`), and the product shows its own name.

import { LANGS } from "./i18n/locales.ts";
import type { PageDocument } from "./i18n/store.ts";

/** The name of the meta tag. */
export const BRAND_META = "obop-brand";

/** The part of the brand that the page carries. */
export type PageBrand = {
  /** The product name people see; never empty. */
  name: string;
  /** The effective slug: `a-z`, `0-9` and `-`, starting with a letter, at most 30. */
  slug: string;
  /** The message under the lead of the sign-in, by language; only languages that have one. */
  messages: Readonly<Partial<Record<(typeof LANGS)[number], string>>>;
  /** The version of the logo, for the cache-busting query of the logo route. */
  v: string;
  /** The theme's stored accent as `#rrggbb`, when it stores one. */
  accent?: string;
};

const SLUG = /^[a-z](?:[a-z0-9-]{0,28}[a-z0-9])?$/;
const ACCENT = /^#[0-9a-fA-F]{6}$/;
const VERSION = /^[0-9a-f]{1,64}$/;

/**
 * The brand a tag's `content` describes, or `null` for anything the server does not write: not JSON,
 * not an object, no name, a slug that breaks the slug rule, a `v` that is not hex. A message in a
 * language we do not ship, or an empty one, is dropped; an accent that is not `#rrggbb` is dropped.
 */
export function parseBrand(content: string | null | undefined): PageBrand | null {
  if (!content) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(content);
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const o = raw as Record<string, unknown>;
  if (typeof o.name !== "string" || o.name.trim() === "") return null;
  if (typeof o.slug !== "string" || !SLUG.test(o.slug)) return null;
  const v = o.v ?? "";
  if (typeof v !== "string" || (v !== "" && !VERSION.test(v))) return null;
  const messages: Partial<Record<(typeof LANGS)[number], string>> = {};
  if (typeof o.messages === "object" && o.messages !== null && !Array.isArray(o.messages)) {
    const m = o.messages as Record<string, unknown>;
    for (const lang of LANGS) {
      const text = Object.hasOwn(m, lang) ? m[lang] : undefined;
      if (typeof text === "string" && text.trim() !== "") messages[lang] = text;
    }
  }
  const brand: PageBrand = { name: o.name, slug: o.slug, messages, v };
  if (typeof o.accent === "string" && ACCENT.test(o.accent)) brand.accent = o.accent.toLowerCase();
  return brand;
}

let last: { content: string; brand: PageBrand | null } | undefined;

/**
 * The brand of this page, read from its meta tag, or `null` (no tag, or one that is not the
 * server's). The result for one `content` is kept, so a component may ask on every render.
 */
export function pageBrand(
  doc: PageDocument | undefined = typeof document === "undefined" ? undefined : document,
): PageBrand | null {
  let content: string | null | undefined;
  try {
    content = doc?.querySelector(`meta[name="${BRAND_META}"]`)?.getAttribute("content");
  } catch {
    return null; // a page that cannot be queried has no tag
  }
  if (!content) return null;
  if (last?.content !== content) last = { content, brand: parseBrand(content) };
  return last.brand;
}

/**
 * The message under the lead of the sign-in, by the chain of the server: the message of the
 * language shown, else the English one, else none (the empty text). A language we do not ship (a
 * pseudo-locale) has no message of its own. The same cases as `Brand::message` in `obop-brand`
 * (`brand.fixtures.json` is read by both tests).
 */
export function brandMessage(brand: PageBrand | null, lang: string): string {
  if (!brand) return "";
  const own = (LANGS as readonly string[]).includes(lang)
    ? brand.messages[lang as (typeof LANGS)[number]]
    : undefined;
  return own || brand.messages.en || "";
}

/** The address of the logo route; `v` makes a new logo a new address. The route is the same in every layout. */
export function brandLogoUrl(brand: Pick<PageBrand, "v">): string {
  return brand.v ? `/api/brand/logo.svg?v=${encodeURIComponent(brand.v)}` : "/api/brand/logo.svg";
}

/** The name of the recovery-codes file: `<slug>-recovery-codes.txt` with a brand, `obop-…` without. */
export function recoveryFileName(brand: Pick<PageBrand, "slug"> | null): string {
  return `${brand ? brand.slug : "obop"}-recovery-codes.txt`;
}
