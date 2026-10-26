/**
 * Instants (D72) in the instance's time zone, with no library: the browser's Intl knows the zone
 * rules. The server stores UTC with six fractional digits (`2026-03-29T01:30:00.000000Z`); people
 * read and type the wall clock of the platform zone, or add an explicit offset or `Z` to say
 * exactly which instant they mean (that is also how the hour that happens twice in autumn is
 * chosen). Nothing here goes through floating point except whole milliseconds of a Date.
 */

/** `new Date(Date.UTC(...))` without the two-digit-year rule of `Date.UTC`. */
function utcMs(y: number, mo: number, d: number, h: number, mi: number, s: number): number {
  const dt = new Date(0);
  dt.setUTCFullYear(y, mo - 1, d);
  dt.setUTCHours(h, mi, s, 0);
  return dt.getTime();
}

// The server takes the years 0001 to 9999. `Date.UTC(1, 0, 1)` would be 1901 (years 0 to 99 mean
// 1900 to 1999 there), so the lower end goes through `utcMs` as well.
const MIN_MS = utcMs(1, 1, 1, 0, 0, 0);
const MAX_MS = utcMs(9999, 12, 31, 23, 59, 59) + 999;

const partsCache = new Map<string, Intl.DateTimeFormat>();
function zoneFormat(zone: string): Intl.DateTimeFormat {
  let f = partsCache.get(zone);
  if (!f) {
    f = new Intl.DateTimeFormat("en-US", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
    partsCache.set(zone, f);
  }
  return f;
}

/** Whether the browser knows the zone. */
export function knownZone(zone: string): boolean {
  try {
    zoneFormat(zone);
    return true;
  } catch {
    return false;
  }
}

/**
 * The wall clock of `zone` at the instant `ms` (a whole second boundary is enough). A zone this
 * browser does not know is read as UTC rather than throwing: the form says so when it matters.
 */
function wall(zone: string, ms: number) {
  const p: Record<string, number> = {};
  for (const x of zoneFormat(knownZone(zone) ? zone : "UTC").formatToParts(new Date(ms)))
    if (x.type !== "literal") p[x.type] = Number(x.value);
  // The era is not asked for: negative years do not occur in 0001 to 9999.
  return { y: p.year!, mo: p.month!, d: p.day!, h: p.hour!, mi: p.minute!, s: p.second! };
}

/** Seconds the wall clock of `zone` is ahead of UTC at the instant `ms`. */
export function offsetSeconds(zone: string, ms: number): number {
  const w = wall(zone, ms);
  const second = Math.floor(ms / 1000) * 1000;
  return Math.round((utcMs(w.y, w.mo, w.d, w.h, w.mi, w.s) - second) / 1000);
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

/** The canonical server text of an instant given in milliseconds and extra microseconds. */
function canonical(ms: number, micros: number): string {
  const d = new Date(ms);
  return (
    `${pad(d.getUTCFullYear(), 4)}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}` +
    `T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}.${pad(micros, 6)}Z`
  );
}

/** Milliseconds and sub-millisecond microseconds of a canonical server text; `null` if malformed. */
function parseCanonical(iso: string): { ms: number; micros: number } | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,6}))?Z$/.exec(iso);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = [m[1], m[2], m[3], m[4], m[5], m[6]].map(Number) as [
    number,
    number,
    number,
    number,
    number,
    number,
  ];
  const frac = (m[7] ?? "").padEnd(6, "0");
  return { ms: utcMs(y, mo, d, h, mi, s) + Number(frac.slice(0, 3)), micros: Number(frac) };
}

/**
 * An instant as it reads in `zone` for an input box: `2026-03-29 03:30` (seconds and a fraction
 * only when they are not zero). Empty for anything that is not a canonical instant.
 */
export function toZonedInput(iso: unknown, zone: string): string {
  if (typeof iso !== "string") return "";
  const p = parseCanonical(iso);
  if (!p) return "";
  const w = wall(zone, Math.floor(p.ms / 1000) * 1000);
  const frac = String(p.micros).padStart(6, "0").replace(/0+$/, "");
  const sec = w.s !== 0 || frac ? `:${pad(w.s)}${frac ? `.${frac}` : ""}` : "";
  return `${pad(w.y, 4)}-${pad(w.mo)}-${pad(w.d)} ${pad(w.h)}:${pad(w.mi)}${sec}`;
}

export type Parsed = { value: string } | { error: "format" | "range" | "gap" | "zone" };

const INPUT =
  /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,6}))?)?\s*(Z|[+-]\d{2}:\d{2})?$/;

/**
 * What was typed as an instant, in the canonical server form. `2026-03-29 03:30` is the wall
 * clock of `zone`; `...Z` or `...+02:00` at the end names the instant outright. An hour that
 * happens twice (clocks going back) is read as its first occurrence; an hour that does not
 * happen (clocks going forward) is an error.
 */
export function fromZonedInput(text: string, zone: string): Parsed {
  const m = INPUT.exec(text.trim());
  if (!m) return { error: "format" };
  const [y, mo, d, h, mi] = [m[1], m[2], m[3], m[4], m[5]].map(Number) as [
    number,
    number,
    number,
    number,
    number,
  ];
  const s = m[6] ? Number(m[6]) : 0;
  const frac = (m[7] ?? "").padEnd(6, "0");
  const micros = Number(frac);
  if (mo < 1 || mo > 12 || h > 23 || mi > 59 || s > 59 || y < 1) return { error: "format" };
  const local = utcMs(y, mo, d, h, mi, s);
  const back = new Date(local);
  if (back.getUTCMonth() + 1 !== mo || back.getUTCDate() !== d) return { error: "format" };
  let ms: number;
  if (m[8]) {
    const off =
      m[8] === "Z"
        ? 0
        : (m[8][0] === "-" ? -1 : 1) *
          (Number(m[8].slice(1, 3)) * 3600 + Number(m[8].slice(4)) * 60);
    if (m[8] !== "Z" && (Number(m[8].slice(1, 3)) > 23 || Number(m[8].slice(4)) > 59))
      return { error: "format" };
    ms = local - off * 1000;
  } else {
    if (!knownZone(zone)) return { error: "zone" };
    // The offset can only be one of the two in force a day before or after.
    const guesses = new Set([
      offsetSeconds(zone, local - 86_400_000),
      offsetSeconds(zone, local + 86_400_000),
    ]);
    const hits = [...guesses]
      .map((o) => local - o * 1000)
      .filter((u) => offsetSeconds(zone, u) === (local - u) / 1000)
      .sort((a, b) => a - b);
    if (hits.length === 0) return { error: "gap" };
    ms = hits[0]!;
  }
  if (ms < MIN_MS || ms > MAX_MS) return { error: "range" };
  return { value: canonical(ms, micros) };
}

/** `YYYY-MM-DD` of an instant on the wall clock of `zone` (the "date in zone" conversion). */
export function dayInZone(iso: unknown, zone: string): string {
  if (typeof iso !== "string") return "";
  const p = parseCanonical(iso);
  if (!p) return "";
  const w = wall(zone, Math.floor(p.ms / 1000) * 1000);
  return `${pad(w.y, 4)}-${pad(w.mo)}-${pad(w.d)}`;
}

const dfCache = new Map<string, Intl.DateTimeFormat>();

/** An instant for display: the date and the time in `zone`, in the locale. */
export function formatInstant(iso: unknown, zone: string, locale: string): string {
  if (typeof iso !== "string") return "";
  const p = parseCanonical(iso);
  if (!p) return String(iso);
  const key = `${locale}|${zone}`;
  let f = dfCache.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, {
      timeZone: knownZone(zone) ? zone : "UTC",
      dateStyle: "medium",
      timeStyle: "medium",
    });
    dfCache.set(key, f);
  }
  return f.format(new Date(p.ms));
}

/** Compare two canonical instants (they are fixed-width text, so text order is time order). */
export const compareInstants = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);
