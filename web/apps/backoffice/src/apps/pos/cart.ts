/**
 * The cart of the Point of Sale register: pure arithmetic, so it can be tested without a browser.
 * The server stays the authority: a line's `subtotal` is computed there by the same formula
 * (`quantity * unit_price * (10000 - discount_bp) / 10000`, whole numbers, a division rounded half
 * away from zero), and the register pays the order's total as the server answers it.
 */

export type CartLine = {
  /** The product's id. */
  product: string;
  name: string;
  /** Price of one unit including tax, in minor units. */
  price: number;
  qty: number;
  /** Discount in basis points: 1000 is 10 %. */
  discountBp: number;
};

export type Method = "cash" | "card" | "voucher" | "bank_transfer" | "other";

/** One payment put on the sale before it is completed. */
export type Tender = { method: Method; amount: number; reference?: string };

/** Integer division rounded half away from zero, as the server's expression language does it. */
export function divRound(a: number, b: number): number {
  const q = Math.abs(a) / Math.abs(b);
  const r = Math.floor(q + 0.5);
  return Math.sign(a) * Math.sign(b) < 0 ? -r : r;
}

export function lineTotal(l: Pick<CartLine, "qty" | "price" | "discountBp">): number {
  return divRound(l.qty * l.price * (10000 - l.discountBp), 10000);
}

export function cartTotal(lines: readonly CartLine[]): number {
  return lines.reduce((s, l) => s + lineTotal(l), 0);
}

export function itemCount(lines: readonly CartLine[]): number {
  return lines.reduce((s, l) => s + l.qty, 0);
}

/** Add one unit of a product: a second tap on the same product raises its quantity. */
export function addProduct(
  lines: readonly CartLine[],
  p: { id: string; name: string; price: number },
): CartLine[] {
  const at = lines.findIndex((l) => l.product === p.id && l.discountBp === 0);
  if (at >= 0) return lines.map((l, i) => (i === at ? { ...l, qty: l.qty + 1 } : l));
  return [...lines, { product: p.id, name: p.name, price: p.price, qty: 1, discountBp: 0 }];
}

/** Change the quantity of line `at`; zero or less removes the line. */
export function setQty(lines: readonly CartLine[], at: number, qty: number): CartLine[] {
  if (qty <= 0) return lines.filter((_, i) => i !== at);
  return lines.map((l, i) => (i === at ? { ...l, qty } : l));
}

/** A discount typed as a percentage ("10", "12,5"), as basis points from 0 to 10000; null if invalid. */
export function percentToBp(s: string): number | null {
  const v = s.trim().replace(",", ".");
  if (v === "") return 0;
  if (!/^\d{1,3}(\.\d{0,2})?$/.test(v)) return null;
  const bp = Math.round(Number(v) * 100);
  return bp >= 0 && bp <= 10000 ? bp : null;
}

export function setDiscount(lines: readonly CartLine[], at: number, bp: number): CartLine[] {
  return lines.map((l, i) => (i === at ? { ...l, discountBp: bp } : l));
}

export function paidSoFar(tenders: readonly Tender[]): number {
  return tenders.reduce((s, x) => s + x.amount, 0);
}

/** What is still to pay: never below zero. */
export function remaining(total: number, tenders: readonly Tender[]): number {
  return Math.max(0, total - paidSoFar(tenders));
}

/**
 * Cash handed over for what is still owed: the payment recorded is what the sale needs, the rest
 * is change. `given` and `owed` are minor units.
 */
export function cashTender(given: number, owed: number): { amount: number; change: number } {
  const amount = Math.min(given, owed);
  return { amount, change: Math.max(0, given - owed) };
}

/**
 * Banknotes a customer is likely to hand over for `owed` (minor units, a currency with two
 * decimals): the exact amount, then the next round amounts above it. At most four, ascending.
 */
export function quickCash(owed: number): number[] {
  if (owed <= 0) return [];
  const out = [owed];
  for (const step of [500, 1000, 2000, 5000, 10000, 20000]) {
    const up = Math.ceil(owed / step) * step;
    if (up > owed && !out.includes(up)) out.push(up);
    if (out.length >= 4) break;
  }
  return out.sort((a, b) => a - b);
}

/** A product matches the search: by name (every word), or by its code (SKU or barcode) exactly. */
export function matches(p: { name: string; sku?: string }, q: string): boolean {
  const s = q.trim().toLowerCase();
  if (!s) return true;
  if (p.sku && p.sku.toLowerCase() === s) return true;
  const hay = `${p.name} ${p.sku ?? ""}`.toLowerCase();
  return s.split(/\s+/).every((w) => hay.includes(w));
}

/** The product a scanner or a typed code names exactly (its SKU), if one does. */
export function byCode<T extends { sku?: string }>(
  products: readonly T[],
  q: string,
): T | undefined {
  const s = q.trim().toLowerCase();
  return s ? products.find((p) => p.sku?.toLowerCase() === s) : undefined;
}
