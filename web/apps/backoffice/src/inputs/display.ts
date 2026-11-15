/**
 * How a value with a display format (presentation.ts `displayOf`, the D95 `ui.display` hint) is
 * read and typed. Pure functions, no React: the stored value never changes shape, only the way a
 * person sees and types it does. Minutes stay minutes, basis points stay basis points, metres stay
 * metres; the inputs translate what was typed back into the stored unit.
 */
import { fmt, t, validDate } from "@obop/ui";
import { canonicalDecimal } from "../decimals.ts";

// ---- durations (minutes) ----------------------------------------------------------------------

/** "45 min", "1 h 30", "62 h": a duration in minutes as a person reads it. */
export function durationText(minutes: unknown): string {
  const n = Number(minutes);
  if (minutes === null || minutes === undefined || minutes === "" || !Number.isFinite(n)) return "";
  const neg = n < 0;
  const m = Math.round(Math.abs(n));
  const h = Math.floor(m / 60);
  const rest = m % 60;
  const text =
    h === 0
      ? t("{minutes} min", { minutes: rest })
      : rest === 0
        ? t("{hours} h", { hours: fmt.integer(h) })
        : t("{hours} h {minutes}", {
            hours: fmt.integer(h),
            minutes: String(rest).padStart(2, "0"),
          });
  return neg ? `−${text}` : text;
}

/** "1:30": the form of a duration that is typed back in. */
export function durationClock(minutes: unknown): string {
  const n = Number(minutes);
  if (minutes === null || minutes === undefined || minutes === "" || !Number.isFinite(n)) return "";
  const m = Math.round(Math.abs(n));
  return `${n < 0 ? "-" : ""}${Math.floor(m / 60)}:${String(m % 60).padStart(2, "0")}`;
}

/**
 * Minutes from what was typed: `1:30`, `1.5h`, `1,5 h`, `1h30`, `1 h 30 min`, `90`, `90m`,
 * `90 min`. `null` when it is not a duration.
 */
export function parseDuration(text: string): number | null {
  const s = text.trim().toLowerCase().replace(/,/g, ".").replace(/\s+/g, " ");
  if (!s) return null;
  let m = /^(\d+):([0-5]?\d)$/.exec(s);
  if (m) return Number(m[1]) * 60 + Number(m[2]);
  m = /^(\d+(?:\.\d+)?) ?(?:h|hr|hrs|hour|hours|std|t)$/.exec(s);
  if (m) return Math.round(Number(m[1]) * 60);
  m = /^(\d+) ?(?:h|hr|hrs|hour|hours|std|t) ?([0-5]?\d) ?(?:m|min|mins|minutes)?$/.exec(s);
  if (m) return Number(m[1]) * 60 + Number(m[2]);
  m = /^(\d+) ?(?:m|min|mins|minute|minutes)?$/.exec(s);
  if (m) return Number(m[1]);
  return null;
}

// ---- time of day (minutes after midnight) -----------------------------------------------------

/** 870 reads as "14:30". */
export function clockText(minutes: unknown): string {
  const n = Number(minutes);
  if (minutes === null || minutes === undefined || minutes === "" || !Number.isInteger(n))
    return "";
  const m = ((n % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

/** "14:30", "14.30", "1430" or "14" as minutes after midnight; `null` otherwise. */
export function parseClock(text: string): number | null {
  const s = text.trim();
  const m = /^(\d{1,2})(?:[:.h]?(\d{2}))?$/.exec(s);
  if (!m) return null;
  const h = Number(m[1]);
  const mi = m[2] ? Number(m[2]) : 0;
  if (h > 23 || mi > 59) return null;
  return h * 60 + mi;
}

// ---- percentages ------------------------------------------------------------------------------

/** 1900 basis points read as "19 %" in the locale. */
export function basisPointsText(bp: unknown): string {
  const n = Number(bp);
  if (bp === null || bp === undefined || bp === "" || !Number.isFinite(n)) return "";
  return fmt.percent(n / 10_000, 2);
}

/** "19", "19,5" or "19.5 %" as basis points (1900, 1950); `null` otherwise. */
export function parseBasisPoints(text: string): number | null {
  const s = text.replace(/%/g, "").trim();
  if (!s) return null;
  const p = fmt.parseNumber(s, 2);
  if (!p || p.ambiguous) return null;
  const n = Number(p.int + p.frac.padEnd(2, "0"));
  return Number.isSafeInteger(n) ? (p.negative ? -n : n) : null;
}

/** A share of 0 to 100 as a fraction of 1 for a bar, clamped. */
export function percentShare(v: unknown): number | null {
  const n = Number(v);
  if (v === null || v === undefined || v === "" || !Number.isFinite(n)) return null;
  return Math.max(0, Math.min(1, n / 100));
}

// ---- distances (metres shown as kilometres) ---------------------------------------------------

/** 200150 metres read as "200.2 km" (one decimal). */
export function kmText(metres: unknown): string {
  const n = Number(metres);
  if (metres === null || metres === undefined || metres === "" || !Number.isFinite(n)) return "";
  return `${fmt.number(n / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} km`;
}

/** Kilometres for an input box: up to three decimals, no grouping, the locale's decimal mark. */
export function kmInput(metres: unknown): string {
  const n = Number(metres);
  if (metres === null || metres === undefined || metres === "" || !Number.isFinite(n)) return "";
  return fmt.number(n / 1000, { maximumFractionDigits: 3, useGrouping: false });
}

/**
 * Kilometres as typed into metres, exactly: the canonical decimal text at `scale` digits when the
 * field is a decimal, or whole metres for a number field. `null` when it is not a number or has
 * more digits than the field keeps.
 */
export function parseKm(text: string, scale: number | null): string | null {
  const s = text.replace(/km/i, "").trim();
  if (!s) return null;
  const keep = (scale ?? 0) + 3;
  const p = fmt.parseNumber(s, keep);
  if (!p || p.ambiguous) return null;
  const frac = p.frac.padEnd(3, "0");
  const int = `${p.int}${frac.slice(0, 3)}`;
  const rest = frac.slice(3);
  if (scale === null) return String(Number(int) * (p.negative ? -1 : 1));
  return canonicalDecimal(p.negative, int, rest, scale);
}

// ---- dates in the region's own order ----------------------------------------------------------

type DateShape = { order: ("d" | "m" | "y")[]; sep: string };
const shapes = new Map<string, DateShape>();

/** The order and separator of a numeric date in the active locale: d.m.y for de-DE. */
export function dateShape(locale = fmt.locale): DateShape {
  let s = shapes.get(locale);
  if (s) return s;
  const order: ("d" | "m" | "y")[] = [];
  let sep = "-";
  try {
    const parts = new Intl.DateTimeFormat(locale, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).formatToParts(new Date(2026, 10, 25));
    for (const p of parts) {
      if (p.type === "day") order.push("d");
      else if (p.type === "month") order.push("m");
      else if (p.type === "year") order.push("y");
      else if (p.type === "literal" && order.length === 1) sep = p.value.trim() || p.value;
    }
  } catch {
    /* an unknown locale reads ISO */
  }
  s = order.length === 3 ? { order, sep } : { order: ["y", "m", "d"], sep: "-" };
  shapes.set(locale, s);
  return s;
}

/** "2026-11-25" as the region writes it with digits: "25.11.2026", "11/25/2026", "2026-11-25". */
export function regionDate(iso: unknown, locale = fmt.locale): string {
  if (typeof iso !== "string" || !validDate(iso)) return iso == null ? "" : String(iso);
  const [y, m, d] = iso.split("-") as [string, string, string];
  const shape = dateShape(locale);
  const by = { y, m, d };
  return shape.order.map((k) => by[k]).join(shape.sep);
}

/** A pattern to show as a placeholder: "DD.MM.YYYY", "MM/DD/YYYY". */
export function regionPattern(locale = fmt.locale): string {
  const shape = dateShape(locale);
  const word = { d: t("DD"), m: t("MM"), y: t("YYYY") };
  return shape.order.map((k) => word[k]).join(shape.sep);
}

const iso = (y: number, m: number, d: number) =>
  `${String(y).padStart(4, "0")}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;

/**
 * A date as typed, in the region's order or as ISO (`2026-11-25`), into ISO; a two-digit year is
 * in this century, a missing year is this year. `null` when it is not a real date.
 */
export function parseRegionDate(
  text: string,
  today = new Date(),
  locale = fmt.locale,
): string | null {
  const s = text.trim();
  if (!s) return null;
  const groups = s.split(/[^\d]+/).filter(Boolean);
  if (groups.length === 1 && /^\d{8}$/.test(groups[0]!)) {
    const g = groups[0]!;
    // 25112026 in the region's order, or 20261125 when it starts with a year.
    const out = /^(19|20)\d{6}$/.test(g)
      ? iso(Number(g.slice(0, 4)), Number(g.slice(4, 6)), Number(g.slice(6)))
      : null;
    if (out && validDate(out)) return out;
  }
  if (groups.length < 2 || groups.length > 3) return null;
  let y: number;
  let m: number;
  let d: number;
  if (groups[0]!.length === 4) {
    if (groups.length !== 3) return null;
    [y, m, d] = groups.map(Number) as [number, number, number];
  } else {
    const order = dateShape(locale).order.filter((k) => groups.length === 3 || k !== "y");
    const by: Record<string, number> = {};
    order.forEach((k, i) => (by[k] = Number(groups[i])));
    y = by.y ?? today.getFullYear();
    if (groups.length === 3 && groups[order.indexOf("y")]!.length <= 2) y += 2000;
    m = by.m!;
    d = by.d!;
  }
  const out = iso(y, m, d);
  return validDate(out) ? out : null;
}

// ---- slugs ------------------------------------------------------------------------------------

/** "Über uns & Kontakt" becomes "uber-uns-kontakt": a slug for a web address. */
export function urlSlug(text: string): string {
  return text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/ß/g, "ss")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
}

/** "Quarterly report.final.pdf" becomes "Quarterly report.final": a title from a file name. */
export function titleFromFileName(name: string): string {
  const base = name.replace(/\.[A-Za-z0-9]{1,6}$/, "");
  return base.replace(/[_]+/g, " ").trim() || name;
}

// ---- links ------------------------------------------------------------------------------------

/** Where a value with a link display points: `tel:`, `mailto:`, a web page or a map search. */
export function linkOf(display: string | undefined, value: unknown): string | null {
  if (typeof value !== "string" || !value.trim()) return null;
  const v = value.trim();
  switch (display) {
    case "tel": {
      const n = v.replace(/[^\d+]/g, "");
      return n.length >= 3 ? `tel:${n}` : null;
    }
    case "email":
      return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v) ? `mailto:${v}` : null;
    case "url":
      if (/^https?:\/\//i.test(v)) return v;
      return /^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(v) ? `https://${v}` : null;
    case "address":
      return `https://www.openstreetmap.org/search?query=${encodeURIComponent(v.replace(/\s*\n\s*/g, ", "))}`;
    default:
      return null;
  }
}
