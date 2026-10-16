// The i18n runtime: the active language, the loaded catalogue, `t()` and the formatters bound to the
// active format locale. A module-level store so that toasts, errors and sort functions can translate
// without a hook; React components re-render through `useI18n()` (see react.tsx).
import { LANGS, LANG_INFO, isLang, isPseudo, matchLang, type Lang } from "./locales.ts";
import { formatNodes, parseMessage, type FormatEnv } from "./message.ts";
import * as N from "./numbers.ts";
import { pseudo } from "./pseudo.ts";

export type Catalogue = Record<string, string>;
/** The result of `import.meta.glob` over the `locales/<lang>/messages.json` files. */
export type Loaders = Record<string, () => Promise<unknown>>;

/** gettext's context separator: the catalogue key of a message with a context. */
export const CTX = "\u0004";

const LS_LANG = "obop-lang";
const LS_REGION = "obop-region";
const COOKIE = "obop_lang";

type State = {
  lang: Lang;
  region: string;
  messages: Catalogue;
  /** Instance defaults from the settings (admins set them). */
  instanceLang: Lang | null;
  currency: string;
  version: number;
};

const S: State = {
  lang: "en",
  region: "auto",
  messages: {},
  instanceLang: null,
  currency: "EUR",
  version: 0,
};

const listeners = new Set<() => void>();
const emit = () => {
  S.version++;
  for (const l of listeners) l();
};
export const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};
export const getVersion = () => S.version;

let loaders: Loaders[] = [];
const loaded = new Map<string, Catalogue>();

/* ---------------- Preferences ---------------- */

function lsGet(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function lsSet(k: string, v: string | null) {
  try {
    if (v === null) localStorage.removeItem(k);
    else localStorage.setItem(k, v);
  } catch {
    /* storage unavailable: the choice lasts until reload */
  }
}

/** `?lang=de` in the address, for tests and shared links. */
export function queryLang(): Lang | null {
  try {
    const v = new URLSearchParams(location.search).get("lang");
    return isLang(v) ? v : matchLang(v);
  } catch {
    return null;
  }
}

/** Does the person have an explicit choice (a saved language)? */
export const personalLang = (): Lang | null => {
  const v = lsGet(LS_LANG);
  return isLang(v) && !isPseudo(v) ? v : null;
};

/** The browser's preferred language that we ship, else English. */
export function browserLang(): Lang {
  const langs =
    typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);
  for (const tag of langs) {
    const m = matchLang(tag);
    if (m && !isPseudo(m)) return m;
  }
  return "en";
}

/** ?lang, then the person's own choice, then the instance default, then the browser, then English. */
export function resolveLang(): Lang {
  return queryLang() ?? personalLang() ?? S.instanceLang ?? browserLang();
}

function writeCookie(lang: Lang) {
  try {
    const secure = location.protocol === "https:" ? "; Secure" : "";
    document.cookie = `${COOKIE}=${lang}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
  } catch {
    /* no cookies */
  }
}

/* ---------------- Loading ---------------- */

async function loadCatalogue(lang: Lang): Promise<Catalogue> {
  if (lang === "en" || isPseudo(lang)) return {};
  const hit = loaded.get(lang);
  if (hit) return hit;
  const merged: Catalogue = {};
  await Promise.all(
    loaders.flatMap((group) =>
      Object.entries(group)
        .filter(([path]) => path.includes(`/${lang}/`))
        .map(async ([, load]) => {
          const mod = (await load()) as Catalogue | { default: Catalogue };
          Object.assign(
            merged,
            "default" in mod && typeof mod.default === "object" ? mod.default : mod,
          );
        }),
    ),
  );
  loaded.set(lang, merged);
  return merged;
}

function applyDocument() {
  if (typeof document === "undefined") return;
  const el = document.documentElement;
  el.lang = S.lang;
  el.dir = LANG_INFO[S.lang].dir;
}

/** Register the lazy catalogues and switch to the resolved language; resolves when it is loaded. */
export async function initI18n(opts: { loaders: Loaders[] }): Promise<void> {
  loaders = opts.loaders;
  S.region = lsGet(LS_REGION) ?? "auto";
  await activate(resolveLang());
  // Keep the server's view of the language current (server-rendered pages read the cookie).
  const own = queryLang() ?? personalLang();
  if (!own) writeCookie(S.lang);
}

async function activate(lang: Lang) {
  const messages = await loadCatalogue(lang);
  S.lang = lang;
  S.messages = messages;
  applyDocument();
  emit();
}

/** The person chooses a language: loaded, applied, remembered and sent to the server as a cookie. */
export async function setLanguage(lang: Lang, persist = true): Promise<void> {
  await activate(lang);
  if (persist && !isPseudo(lang)) {
    lsSet(LS_LANG, lang);
    writeCookie(lang);
  }
}

/** Forget the personal choice; the instance default or the browser decides again. */
export async function resetLanguage(): Promise<void> {
  lsSet(LS_LANG, null);
  await activate(resolveLang());
  writeCookie(S.lang);
}

export function setRegion(region: string) {
  S.region = region;
  lsSet(LS_REGION, region === "auto" ? null : region);
  emit();
}

/** Settings of the instance: the default language (used until the person picks one) and currency. */
export async function setInstanceDefaults(d: { language?: string; currency?: string }) {
  const lang = matchLang(d.language);
  const currency = /^[A-Z]{3}$/.test(d.currency ?? "") ? d.currency! : "EUR";
  const langChanged = (lang ?? null) !== S.instanceLang;
  S.instanceLang = lang && !isPseudo(lang) ? lang : null;
  if (currency !== S.currency) {
    S.currency = currency;
    emit();
  }
  if (langChanged && !queryLang() && !personalLang()) await activate(resolveLang());
}

/* ---------------- Active state ---------------- */

export const getLang = () => S.lang;
export const getRegion = () => S.region;
export const getCurrency = () => S.currency;
export const getDir = () => LANG_INFO[S.lang].dir;
export const instanceLang = () => S.instanceLang;

/** The locale used for numbers, dates and currency: the chosen region, else the browser's, else the language's. */
export function formatLocale(lang: Lang = S.lang, region: string = S.region): string {
  if (region !== "auto") return region;
  const nav = typeof navigator === "undefined" ? "" : navigator.language;
  const base = (x: string) => x.split("-")[0];
  const uiBase = isPseudo(lang) ? "en" : lang;
  if (nav && base(nav) === base(uiBase)) return nav;
  return LANG_INFO[lang].format;
}

const pluralRules = new Map<string, Intl.PluralRules>();
function pluralOf(n: number): Intl.LDMLPluralRule {
  const code = isPseudo(S.lang) ? "en" : S.lang;
  let r = pluralRules.get(code);
  if (!r) pluralRules.set(code, (r = new Intl.PluralRules(code)));
  return r.select(n);
}

/* ---------------- Messages ---------------- */

function listFormat(v: unknown, style?: string): string {
  if (Array.isArray(v)) return N.formatList(v.map(String), formatLocale(), "conjunction");
  if (v instanceof Date) {
    return style === "date:short" || !style
      ? N.df(formatLocale(), { dateStyle: "medium" }).format(v)
      : N.formatDateTime(v, formatLocale());
  }
  return String(v);
}

function numberStyled(n: number, style?: string): string {
  const loc = formatLocale();
  if (style === "number:percent") return N.nf(loc, { style: "percent" }).format(n);
  if (style === "number:integer") return N.nf(loc, { maximumFractionDigits: 0 }).format(n);
  return N.nf(loc).format(n);
}

const STRING_ENV: FormatEnv<string> = {
  plural: pluralOf,
  number: numberStyled,
  value: listFormat,
  text: (s) => (isPseudo(S.lang) ? pseudo(s) : s),
  join: (parts) => parts.join(""),
};

/** Where a translation is looked up. `ctx` separates words that translate differently. */
const keyOf = (id: string, ctx?: string) => (ctx ? `${ctx}${CTX}${id}` : id);

/** The English text with the message's own syntax removed, for when nothing is translated. */
const needsParse = (s: string) => /[{'<]/.test(s);

type Primitive = string | number | boolean | null | undefined | Date | string[];

/** Names of `{placeholders}` in a message literal; plural branches (`one {# x}`) are not names. */
type NamesOf<S extends string> = S extends `${string}{${infer Body}}${infer Rest}`
  ? (Body extends `${infer N},${string}` ? Trim<N> : Trim<Body>) extends infer V
    ? (V extends `#${string}` | "" | `${string} ${string}` ? never : V) | NamesOf<Rest>
    : never
  : never;
type Trim<S extends string> = S extends ` ${infer R}`
  ? Trim<R>
  : S extends `${infer R} `
    ? Trim<R>
    : S;

export type Vars<S extends string> = string extends S
  ? Record<string, Primitive>
  : [NamesOf<S>] extends [never]
    ? Record<string, never>
    : { [K in NamesOf<S>]: Primitive };

// Plural and select messages nest braces, which the type-level scan cannot follow, so their
// variables are checked at runtime by the catalogue check instead.
type VarsArg<S extends string> = string extends S
  ? [vars?: Record<string, Primitive>]
  : S extends `${string}${"plural," | "select,"}${string}`
    ? [vars: Record<string, Primitive>]
    : [NamesOf<S>] extends [never]
      ? [vars?: undefined]
      : [vars: Vars<S>];

/** Translate. The English text is the message id; `{name}` and ICU plurals are supported. */
export function t<S extends string>(id: S, ...args: VarsArg<S>): string {
  return translate(id, undefined, args[0] as Record<string, unknown> | undefined);
}

/** Translate with a context ("field", "model", "app", "option") when one English word has several senses. */
export function tx<S extends string>(ctx: string, id: S, ...args: VarsArg<S>): string {
  return translate(id, ctx, args[0] as Record<string, unknown> | undefined);
}

export function translate(id: string, ctx?: string, vars?: Record<string, unknown>): string {
  const hit = S.messages[keyOf(id, ctx)];
  const text = hit ?? id;
  if (!vars && (hit === undefined || !needsParse(text)) && !isPseudo(S.lang)) return text;
  return formatNodes(parseMessage(text), vars ?? {}, STRING_ENV);
}

/** The catalogue text of a message with its ICU syntax intact (English: the id itself). */
export const rawMessage = (id: string, ctx?: string): string => S.messages[keyOf(id, ctx)] ?? id;

/** Is there a real translation (or is this English / pseudo)? For `labelFor` fallbacks. */
export const hasTranslation = (id: string, ctx?: string) =>
  S.lang === "en" || isPseudo(S.lang) || keyOf(id, ctx) in S.messages;

/** Marks a message for extraction without translating it now; translate later with `t(id)`. */
export const msg = <S extends string>(id: S): S => id;
/** Like `msg` for a message with a context; translate later with `tx(ctx, id)`. */
export const msgx = <S extends string>(_ctx: string, id: S): S => id;

/* ---------------- Rich text ---------------- */

export { STRING_ENV as stringEnv, keyOf, pluralOf };

/* ---------------- Formatters bound to the active locale ---------------- */

export const fmt = {
  get locale() {
    return formatLocale();
  },
  number: (n: unknown, o?: Intl.NumberFormatOptions) => N.formatNumber(n, formatLocale(), o),
  integer: (n: unknown) => N.formatInteger(n, formatLocale()),
  percent: (n: number, digits = 1) =>
    N.nf(formatLocale(), { style: "percent", maximumFractionDigits: digits }).format(n),
  /** Format a Date with Intl options in the active format locale. */
  dateOf: (d: Date, o: Intl.DateTimeFormatOptions) => N.df(formatLocale(), o).format(d),
  /** First day of the week in the active format locale, 1 = Monday ... 7 = Sunday. */
  firstDay: (): number => {
    try {
      const l = new Intl.Locale(formatLocale()) as Intl.Locale & {
        getWeekInfo?: () => { firstDay: number };
        weekInfo?: { firstDay: number };
      };
      return (l.getWeekInfo?.() ?? l.weekInfo)?.firstDay ?? 1;
    } catch {
      return 1;
    }
  },
  money: (minor: unknown, currency?: string) =>
    N.formatMoney(minor, currency ?? S.currency, formatLocale()),
  /** The amount as typed in an edit field ("12,50" for a German locale). */
  moneyInput: (minor: unknown, currency?: string) =>
    N.moneyInputText(minor, currency ?? S.currency, formatLocale()),
  parseMoney: (text: string, currency?: string) =>
    N.parseMoney(text, currency ?? S.currency, formatLocale()),
  parseNumber: (text: string, maxFrac = 2) => N.parseNumber(text, formatLocale(), maxFrac),
  exponent: (currency?: string) => N.currencyExponent(currency ?? S.currency, formatLocale()),
  date: (s: unknown, o?: Intl.DateTimeFormatOptions) => N.formatPlainDate(s, formatLocale(), o),
  month: (key: string) => N.formatMonth(key, formatLocale()),
  dateTime: (v: Date | number | string) => N.formatDateTime(v, formatLocale()),
  relative: (v: Date | number | string, now?: number) => N.formatRelative(v, formatLocale(), now),
  list: (items: string[], type?: "conjunction" | "disjunction") =>
    N.formatList(items, formatLocale(), type),
  /** Decimal and group separators of the active format locale. */
  separators: () => N.separators(formatLocale()),
};

let collator: { loc: string; c: Intl.Collator } | null = null;
/** Collation-aware comparison for client-side sorts (accents, case and numbers in the active locale). */
export function compareText(a: string, b: string): number {
  const loc = formatLocale();
  if (!collator || collator.loc !== loc)
    collator = { loc, c: new Intl.Collator(loc, { numeric: true, sensitivity: "base" }) };
  return collator.c.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0);
}

export { LANGS };
