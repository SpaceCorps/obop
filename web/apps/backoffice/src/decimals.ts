/**
 * Exact decimals (D72) in the browser. The server sends and takes a decimal as a canonical string
 * (`"12.3400"`, `"-0.05"`); nothing here turns one into a floating point number. Comparison and
 * sums work on scaled `BigInt`s, display hands the digits to `Intl.NumberFormat` as a string,
 * which it formats exactly.
 */

const CANONICAL = /^(-?)(\d+)(?:\.(\d+))?$/;

/** Whether `s` is a decimal in the server's text form (also what a shared link holds). */
export const isDecimalText = (s: unknown): s is string =>
  typeof s === "string" && CANONICAL.test(s);

/** Sign, integer digits and fraction digits of a canonical decimal; `null` for anything else. */
export function splitDecimal(s: string): { neg: boolean; int: string; frac: string } | null {
  const m = CANONICAL.exec(s);
  return m ? { neg: m[1] === "-", int: m[2]!, frac: m[3] ?? "" } : null;
}

/** The canonical text of typed parts: `scale` fraction digits, no leading zeros, no `-0`. */
export function canonicalDecimal(neg: boolean, int: string, frac: string, scale: number): string {
  const i = int.replace(/^0+(?=\d)/, "") || "0";
  const f = frac.padEnd(scale, "0");
  const zero = /^0*$/.test(i) && /^0*$/.test(f);
  return `${neg && !zero ? "-" : ""}${i}${scale > 0 ? `.${f}` : ""}`;
}

/** Whole units at `scale` digits after the point: "12.5" at 3 is 12500n. */
function scaled(s: string, scale: number): bigint {
  const p = splitDecimal(s);
  if (!p) return 0n;
  const n = BigInt(p.int + p.frac.padEnd(scale, "0").slice(0, scale));
  return p.neg ? -n : n;
}

const scaleOf = (xs: readonly string[]): number => {
  let s = 0;
  for (const x of xs) s = Math.max(s, splitDecimal(x)?.frac.length ?? 0);
  return s;
};

/** Numeric order of two decimals, exact: "9.5" < "10.25". */
export function compareDecimals(a: string, b: string): number {
  const s = scaleOf([a, b]);
  const x = scaled(a, s);
  const y = scaled(b, s);
  return x < y ? -1 : x > y ? 1 : 0;
}

/** The exact sum of decimals, written with the largest scale among them. */
export function sumDecimals(values: readonly string[]): string {
  const s = scaleOf(values);
  let total = 0n;
  for (const v of values) total += scaled(v, s);
  const neg = total < 0n;
  const digits = (neg ? -total : total).toString().padStart(s + 1, "0");
  return canonicalDecimal(
    neg,
    s > 0 ? digits.slice(0, -s) : digits,
    s > 0 ? digits.slice(-s) : "",
    s,
  );
}

/** A decimal for reading, in the locale: grouped, with the locale's decimal mark, `scale` digits. */
export function formatDecimal(value: unknown, locale: string, scale?: number): string {
  if (typeof value !== "string" || !isDecimalText(value)) return value == null ? "" : String(value);
  const digits = scale ?? splitDecimal(value)!.frac.length;
  const f = new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  // A string is formatted as the exact decimal it spells (no trip through a float).
  return f.format(value as unknown as number);
}

/** A decimal for an input box: the locale's decimal mark and no grouping. */
export function decimalInput(value: unknown, decimalMark: string): string {
  return typeof value === "string" && isDecimalText(value)
    ? value.replace(".", decimalMark)
    : value == null
      ? ""
      : String(value);
}
