// The layout of the Swedish VAT return (momsdeklaration, form SKV 4700) as sections of box
// numbers, and the amount of each box from the report's lines. UNVERIFIED like the package: the
// grouping follows the form as published, and a box this list does not know goes under "Other".
import type { Row } from "../../api.ts";

export type SectionKey = "A" | "B" | "C" | "D" | "H" | "I" | "E" | "F" | "G" | "other";

/** The sections in the order of the form, with the box numbers each holds. */
export const SECTIONS: { key: SectionKey; boxes: string[] }[] = [
  { key: "A", boxes: ["05", "06", "07", "08"] },
  { key: "B", boxes: ["10", "11", "12"] },
  { key: "C", boxes: ["20", "21", "22", "23", "24"] },
  { key: "D", boxes: ["30", "31", "32"] },
  { key: "H", boxes: ["50"] },
  { key: "I", boxes: ["60", "61", "62"] },
  { key: "E", boxes: ["35", "36", "37", "38", "39", "40", "41", "42"] },
  { key: "F", boxes: ["48"] },
  { key: "G", boxes: ["49"] },
];

/** "5" and "05" are the same box. */
export const boxKey = (v: unknown): string => {
  const s = String(v ?? "").trim();
  return /^\d$/.test(s) ? `0${s}` : s;
};

export type BoxRow = {
  id: string;
  box: string;
  label: string;
  amount: number;
  /** The report has a line for the box (an amount of 0 is still an answer). */
  filled: boolean;
  verified: boolean;
  /** The vouchers behind the amount, one per line, as the computation wrote them. */
  vouchers: string;
};

export type ReturnSection = { key: SectionKey; rows: BoxRow[] };

/**
 * The return's sections with their boxes: every box of the configuration that is in use or has a
 * line on the report, its amount the sum of the report's lines for it. Sections with no box are
 * left out. Box 49 is not taken from the lines: it is the report's own `vat_to_pay`.
 */
export function returnSections(
  boxes: readonly Row[],
  lines: readonly Row[],
  vatToPay: number,
): ReturnSection[] {
  const byBox = new Map<string, { amount: number; vouchers: string[] }>();
  for (const l of lines) {
    const id = String(l.box_id ?? "");
    const had = byBox.get(id) ?? { amount: 0, vouchers: [] };
    had.amount += typeof l.amount === "number" ? l.amount : 0;
    if (typeof l.vouchers === "string" && l.vouchers.trim()) had.vouchers.push(l.vouchers.trim());
    byBox.set(id, had);
  }
  const rows: BoxRow[] = boxes
    .map((b) => {
      const hit = byBox.get(b.id);
      const box = boxKey(b.box);
      return {
        id: b.id,
        box,
        label: String(b.label ?? ""),
        amount: box === "49" ? vatToPay : (hit?.amount ?? 0),
        filled: box === "49" || !!hit,
        verified: b.review === "verified" || b.verified === true,
        vouchers: hit?.vouchers.join("\n") ?? "",
        used: b.not_used !== true,
      };
    })
    .filter((r) => r.used || r.filled)
    .map(({ used: _used, ...r }) => r);
  const known = new Set(SECTIONS.flatMap((s) => s.boxes));
  const out: ReturnSection[] = SECTIONS.map((s) => ({
    key: s.key,
    rows: rows
      .filter((r) => s.boxes.includes(r.box))
      .sort((a, b) => a.box.localeCompare(b.box, undefined, { numeric: true })),
  }));
  out.push({
    key: "other",
    rows: rows
      .filter((r) => !known.has(r.box))
      .sort((a, b) => a.box.localeCompare(b.box, undefined, { numeric: true })),
  });
  return out.filter((s) => s.rows.length > 0);
}
