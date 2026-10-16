// Locale-aware number, money and date formatting and parsing. Pure functions that take the format
// locale explicitly; the bound versions that follow the active language live in store.ts.
// D9: money is an integer in minor units and is never put through floating point on the way in;
// amounts are handled as decimal digit strings, which Intl.NumberFormat formats exactly.

const nfCache = new Map<string, Intl.NumberFormat>();
export function nf(locale: string, o: Intl.NumberFormatOptions = {}): Intl.NumberFormat {
  const key = locale + JSON.stringify(o);
  let f = nfCache.get(key);
  if (!f) nfCache.set(key, (f = new Intl.NumberFormat(locale, o)));
  return f;
}

const dfCache = new Map<string, Intl.DateTimeFormat>();
export function df(locale: string, o: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = locale + JSON.stringify(o);
  let f = dfCache.get(key);
  if (!f) dfCache.set(key, (f = new Intl.DateTimeFormat(locale, o)));
  return f;
}

/** Decimal digits of a currency (2 for EUR, 0 for JPY, 3 for BHD). */
export function currencyExponent(currency: string, locale = "en"): number {
  try {
    return nf(locale, { style: "currency", currency }).resolvedOptions().maximumFractionDigits ?? 2;
  } catch {
    return 2;
  }
}

/** Separators of a locale, from CLDR through Intl. */
export function separators(locale: string): { decimal: string; group: string } {
  const parts = nf(locale).formatToParts(11111.1);
  return {
    decimal: parts.find((p) => p.type === "decimal")?.value ?? ".",
    group: parts.find((p) => p.type === "group")?.value ?? ",",
  };
}

/** An integer of minor units as an exact decimal string: (1250, 2) -> "12.50". */
export function minorToDecimal(minor: number | bigint, exponent: number): string {
  const neg = minor < 0;
  let digits = (neg ? -BigInt(minor) : BigInt(minor)).toString();
  if (exponent > 0) {
    digits = digits.padStart(exponent + 1, "0");
    digits = `${digits.slice(0, -exponent)}.${digits.slice(-exponent)}`;
  }
  return (neg ? "-" : "") + digits;
}

/** `null` for anything that is not a finite number. */
function toMinor(minor: unknown): number | null {
  if (typeof minor === "string" && minor.trim() === "") return null;
  if (minor === null || minor === undefined || typeof minor === "boolean") return null;
  const n = Number(minor);
  return Number.isFinite(n) ? n : null;
}

export function formatMoney(minor: unknown, currency: string, locale: string): string {
  const n = toMinor(minor);
  if (n === null) return "—";
  const exp = currencyExponent(currency, locale);
  try {
    return nf(locale, { style: "currency", currency }).format(
      minorToDecimal(Math.trunc(n), exp) as unknown as number,
    );
  } catch {
    return `${minorToDecimal(Math.trunc(n), exp)} ${currency}`;
  }
}

/** The amount for an edit field: locale decimal separator, no grouping, always the full exponent. */
export function moneyInputText(minor: unknown, currency: string, locale: string): string {
  const n = toMinor(minor);
  if (n === null) return "";
  const exp = currencyExponent(currency, locale);
  return nf(locale, {
    useGrouping: false,
    minimumFractionDigits: exp,
    maximumFractionDigits: exp,
  }).format(minorToDecimal(Math.trunc(n), exp) as unknown as number);
}

export function formatInteger(n: unknown, locale: string): string {
  const v = toMinor(n);
  return v === null ? "—" : nf(locale, { maximumFractionDigits: 0 }).format(v);
}

export function formatNumber(n: unknown, locale: string, o?: Intl.NumberFormatOptions): string {
  const v = toMinor(n);
  return v === null ? "—" : nf(locale, o).format(v);
}

export type ParsedNumber = {
  negative: boolean;
  /** Integer digits, no separators ("0" when empty). */
  int: string;
  /** Fraction digits, possibly empty. */
  frac: string;
  /** The typed separator could be a group or a decimal mark; the caller shows the reading before commit. */
  ambiguous: boolean;
};

const ARABIC_DIGITS = /[٠-٩۰-۹]/g;

/**
 * Read a typed number in the person's locale. Accepts `.` and `,` (the numpad inserts whichever the
 * keyboard has), spaces, NBSP, narrow NBSP and apostrophes as group separators, full-width digits and
 * the Unicode minus. `maxFrac` is the most fraction digits the field allows (money: the currency's
 * exponent). Returns `null` when the text is not a number or has too many decimals.
 */
export function parseNumber(
  input: string,
  locale: string,
  maxFrac = 2,
  strip: string[] = [],
): ParsedNumber | null {
  let s = input.normalize("NFKC").trim();
  s = s.replace(ARABIC_DIGITS, (d) => String(d.charCodeAt(0) & 0xf)).replace(/[−–—]/g, "-");
  for (const t of strip) if (t) s = s.split(t).join(" ");
  s = s.trim();
  let negative = false;
  if (s.startsWith("-")) {
    negative = true;
    s = s.slice(1).trim();
  } else if (s.startsWith("+")) s = s.slice(1).trim();
  if (!s) return null;
  // Spaces and apostrophes (Swiss) separate groups: "1 250,5", "1'250.50".
  if (/[\s'’ʼ]/.test(s)) {
    const groups = s.split(/[\s'’ʼ]+/);
    const last = groups[groups.length - 1]!;
    const ok =
      groups.length >= 2 &&
      /^\d{1,3}$/.test(groups[0]!) &&
      groups.slice(1, -1).every((g) => /^\d{3}$/.test(g)) &&
      /^\d{3}([.,]\d*)?$/.test(last);
    if (!ok) return null;
    s = groups.join("");
  }
  if (!/^[\d.,]+$/.test(s) || !/\d/.test(s)) return null;

  const loc = separators(locale);
  const dots = s.split(".").length - 1;
  const commas = s.split(",").length - 1;
  const groupsOf3 = (x: string, sep: string) => {
    const p = x.split(sep);
    return p.every((g, i) => (i === 0 ? /^\d{1,3}$/.test(g) : /^\d{3}$/.test(g)));
  };
  let int: string;
  let frac = "";
  let ambiguous = false;
  const finish = (): ParsedNumber | null =>
    frac.length > maxFrac
      ? null
      : { negative, int: int.replace(/^0+(?=\d)/, "") || "0", frac, ambiguous };

  if (dots && commas) {
    const dec = s.lastIndexOf(".") > s.lastIndexOf(",") ? "." : ",";
    const grp = dec === "." ? "," : ".";
    if (s.split(dec).length - 1 !== 1) return null;
    const [a, b] = s.split(dec) as [string, string];
    if (!groupsOf3(a, grp)) return null;
    int = a.split(grp).join("");
    frac = b;
    return finish();
  }
  if (!dots && !commas) {
    int = s;
    return finish();
  }
  const sep = dots ? "." : ",";
  const count = dots || commas;
  if (count > 1) {
    if (!groupsOf3(s, sep)) return null;
    int = s.split(sep).join("");
    return finish();
  }
  const [a, b] = s.split(sep) as [string, string];
  if (b === "") return null;
  const looksGrouped = b.length === 3 && /^\d{1,3}$/.test(a) && !/^0\d*$/.test(a);
  const asGroup = () => {
    int = a + b;
    frac = "";
    return finish();
  };
  const asDecimal = () => {
    int = a;
    frac = b;
    return finish();
  };
  if (!looksGrouped) return asDecimal();
  if (sep === loc.group) return asGroup();
  // Three digits after a mark that is not this locale's group separator: a decimal when the field
  // takes three decimals, else a group (the reading is shown before commit).
  if (sep === loc.decimal && maxFrac >= 3) return asDecimal();
  if (maxFrac >= 3) {
    ambiguous = true;
    return asDecimal();
  }
  ambiguous = true;
  return asGroup();
}

export type ParsedMoney = { minor: number; ambiguous: boolean };

/** A typed amount in major units ("12,50", "1.250,00 €") as integer minor units. */
export function parseMoney(input: string, currency: string, locale: string): ParsedMoney | null {
  const exp = currencyExponent(currency, locale);
  const strip = [currency, ...currencyTokens(currency, locale)];
  const p = parseNumber(input, locale, exp, strip);
  if (!p) return null;
  const digits = p.int + p.frac.padEnd(exp, "0");
  const n = Number(digits);
  // An amount beyond 2^53 is still sent: the server decides what fits and says so on the field.
  if (!Number.isFinite(n)) return null;
  return { minor: p.negative && n !== 0 ? -n : n, ambiguous: p.ambiguous };
}

function currencyTokens(currency: string, locale: string): string[] {
  const out = new Set<string>(["€", "$", "£", "¥"]);
  for (const display of ["symbol", "narrowSymbol", "code"] as const) {
    try {
      const part = nf(locale, { style: "currency", currency, currencyDisplay: display })
        .formatToParts(1)
        .find((p) => p.type === "currency");
      if (part) out.add(part.value);
    } catch {
      /* unknown currency code */
    }
  }
  return [...out].sort((a, b) => b.length - a.length);
}

/* ================= Dates ================= */

export function validDate(s: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(s)) return false;
  const [y, m, d] = s.split("-").map(Number) as [number, number, number];
  const dt = new Date(y, m - 1, d);
  return dt.getFullYear() === y && dt.getMonth() === m - 1 && dt.getDate() === d;
}

/** A plain accounting date ("2026-03-05"), never shifted by a time zone. */
export function formatPlainDate(
  s: unknown,
  locale: string,
  o: Intl.DateTimeFormatOptions = { year: "numeric", month: "short", day: "numeric" },
): string {
  if (typeof s !== "string" || !validDate(s)) return s ? String(s) : "—";
  const [y, m, d] = s.split("-").map(Number) as [number, number, number];
  return df(locale, o).format(new Date(y, m - 1, d));
}

export function formatMonth(key: string, locale: string): string {
  const [y, m] = key.split("-").map(Number);
  if (!y || !m) return "";
  return df(locale, { year: "numeric", month: "long" }).format(new Date(y, m - 1, 1));
}

export function formatDateTime(v: Date | number | string, locale: string): string {
  const d = v instanceof Date ? v : new Date(v);
  if (Number.isNaN(d.getTime())) return "—";
  return df(locale, { dateStyle: "medium", timeStyle: "short" }).format(d);
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 86400],
  ["month", 30 * 86400],
  ["week", 7 * 86400],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
  ["second", 1],
];

/** "3 days ago", "in 2 hours", "yesterday" in the locale. */
export function formatRelative(
  v: Date | number | string,
  locale: string,
  now = Date.now(),
): string {
  const t = v instanceof Date ? v.getTime() : typeof v === "number" ? v : new Date(v).getTime();
  if (Number.isNaN(t)) return "—";
  const diff = Math.round((t - now) / 1000);
  const abs = Math.abs(diff);
  const [unit, secs] = UNITS.find(([, s]) => abs >= s) ?? UNITS[UNITS.length - 1]!;
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(
    Math.trunc(diff / secs),
    unit,
  );
}

export function formatList(
  items: string[],
  locale: string,
  type: "conjunction" | "disjunction" = "conjunction",
): string {
  return new Intl.ListFormat(locale, { style: "long", type }).format(items);
}
