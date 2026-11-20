// The named formats of a text field (K45, `pattern` in /api/meta), checked as the server checks
// them (`obop-registry` limits.rs `Pattern::matches`). The tables and the cases are in
// patterns.fixtures.json, which a server test reads too, so the two cannot drift (U18).
import { t } from "@obop/ui";
import { getServerCurrencies } from "./api.ts";
import data from "./patterns.fixtures.json";

export type Pattern = "currency_code" | "iban" | "vat_id";

const IBAN_LENGTHS: Record<string, number> = data.iban_lengths;
const VAT_SHAPES: Record<string, string> = data.vat_shapes;

const isUpper = (c: string) => c >= "A" && c <= "Z";
const isDigit = (c: string) => c >= "0" && c <= "9";
/** Whether every character of `s` passes `ok` (a string has no `every`). */
const all = (s: string, ok: (c: string) => boolean) => Array.from(s).every((c) => ok(c));

function ibanOk(s: string): boolean {
  const c = s.replaceAll(" ", "").toUpperCase();
  if (c.length < 15 || c.length > 34) return false;
  if (!all(c.slice(0, 2), isUpper) || !all(c.slice(2, 4), isDigit)) return false;
  if (!all(c, (x) => isUpper(x) || isDigit(x))) return false;
  const n = IBAN_LENGTHS[c.slice(0, 2)];
  if (n !== undefined && c.length !== n) return false;
  // ISO 7064 mod 97-10 over the account, then the country and check digits, letters as 10..35.
  let rem = 0;
  for (const x of c.slice(4) + c.slice(0, 4))
    rem = isDigit(x) ? (rem * 10 + Number(x)) % 97 : (rem * 100 + x.charCodeAt(0) - 55) % 97;
  return rem === 1;
}

function vatOk(s: string): boolean {
  const c = s.replaceAll(" ", "");
  if (c.length < 4 || !all(c.slice(0, 2), isUpper)) return false;
  const rest = c.slice(2);
  const shapes = VAT_SHAPES[c.slice(0, 2)];
  if (shapes === undefined)
    return rest.length >= 2 && rest.length <= 13 && all(rest, (x) => isUpper(x) || isDigit(x));
  return shapes.split("|").some(
    (shape) =>
      shape.length === rest.length &&
      Array.from(shape).every((p, i) => {
        const b = rest[i];
        if (p === "9") return isDigit(b);
        if (p === "A") return isUpper(b);
        if (p === "X") return isUpper(b) || isDigit(b);
        return p === b;
      }),
  );
}

/** Whether `s` has the format; the empty text is left to `required`. A currency code is checked
 * against the server's list when the server sent one, else it is not checked here. */
export function matchesPattern(pattern: string, s: string): boolean {
  if (s === "") return true;
  switch (pattern) {
    case "iban":
      return ibanOk(s);
    case "vat_id":
      return vatOk(s);
    case "currency_code": {
      const list = getServerCurrencies();
      return list.length === 0 || list.includes(s);
    }
    default:
      return true;
  }
}

/** The name of a format in words ("IBAN") and an example of a valid value, as the server's. */
export function patternWords(pattern: string): { what: string; example: string } {
  switch (pattern) {
    case "iban":
      return { what: "IBAN", example: "DE89 3704 0044 0532 0130 00" };
    case "vat_id":
      return { what: t("VAT number"), example: "DE123456789" };
    case "currency_code":
      return { what: t("currency code"), example: "EUR" };
    default:
      return { what: pattern, example: "" };
  }
}
