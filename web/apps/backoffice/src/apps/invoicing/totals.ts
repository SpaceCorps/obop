import type { Row } from "../../api.ts";

const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);

/**
 * The amount of each invoice line and their sum, in minor units. A line's amount is the server's
 * computed `amount_minor`; a line that has none yet (an older row) is quantity times unit price,
 * the rule the module computes it with.
 */
export function invoiceTotals(lines: readonly Row[]): {
  total: number;
  amountOf: (line: Row) => number;
} {
  const amountOf = (l: Row) =>
    typeof l.amount_minor === "number" ? l.amount_minor : num(l.quantity) * num(l.unit_price_minor);
  return { total: lines.reduce((n, l) => n + amountOf(l), 0), amountOf };
}
