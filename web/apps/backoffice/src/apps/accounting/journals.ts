// The figures of the Accounting overview's journal cards, made from what `<model>.search` answers
// (rows and `aggregate` groups, D4). Pure, so the arithmetic is tested without a server.
import type { AggGroup, Row } from "../../api.ts";

export type JournalKind = "bank" | "cash" | "sale" | "purchase" | "general";

/** One journal card: what waits in the journal and, for a bank or cash journal, its statements. */
export type JournalCard = {
  id: string;
  name: string;
  code: string;
  kind: JournalKind;
  currency: string;
  drafts: { count: number; sum: number };
  posted: { count: number; sum: number };
  /** Bank and cash journals: the statement lines not matched yet. */
  toMatch: { count: number; sum: number };
  /** Bank and cash journals: debit minus credit of the posted entries on the journal's account. */
  ledgerBalance: number;
  statementLines: number;
};

/** Bank and cash first (where the daily work is), then sales, purchases and the rest. */
const ORDER: JournalKind[] = ["bank", "cash", "sale", "purchase", "general"];

const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);

/**
 * The cards of the active journals, in the order bank, cash, sale, purchase, general, then by code.
 * `moves` is grouped by journal_id and state with `count` and `sum:total_debit`; `bank` by
 * journal_id and state with `count` and `sum:amount`.
 */
export function journalCards(
  journals: readonly Row[],
  moves: readonly AggGroup[],
  bank: readonly AggGroup[],
  instanceCurrency: string,
  ledger: ReadonlyMap<string, number> = new Map(),
): JournalCard[] {
  const cards = journals
    .filter((j) => j.active !== false)
    .map((j): JournalCard => {
      const kind = (ORDER as string[]).includes(String(j.kind))
        ? (j.kind as JournalKind)
        : "general";
      const mine = (gs: readonly AggGroup[]) => gs.filter((g) => g.key.journal_id === j.id);
      const sumOf = (gs: readonly AggGroup[], state: string, measure: string) => {
        const hit = gs.filter((g) => g.key.state === state);
        return {
          count: hit.reduce((n, g) => n + g.count, 0),
          sum: hit.reduce((n, g) => n + num(g.measures[measure]), 0),
        };
      };
      const m = mine(moves);
      const b = mine(bank);
      const cur = typeof j.currency === "string" && /^[A-Z]{3}$/.test(j.currency);
      return {
        id: j.id,
        name: String(j.name ?? j.code ?? ""),
        code: String(j.code ?? ""),
        kind,
        currency: cur ? String(j.currency) : instanceCurrency,
        drafts: sumOf(m, "draft", "sum:total_debit"),
        posted: sumOf(m, "posted", "sum:total_debit"),
        toMatch: sumOf(b, "unmatched", "sum:amount"),
        ledgerBalance: ledger.get(String(j.default_account_id ?? "")) ?? 0,
        statementLines: b.reduce((n, g) => n + g.count, 0),
      };
    });
  return cards.sort(
    (a, b) =>
      ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind) ||
      a.code.localeCompare(b.code) ||
      a.name.localeCompare(b.name),
  );
}

/** Whether the card is a bank or cash journal: it has statements to reconcile. */
export const hasStatements = (c: Pick<JournalCard, "kind">): boolean =>
  c.kind === "bank" || c.kind === "cash";

/**
 * Debit minus credit per account over the entries that count in the ledger. `groups` is
 * `accounting_move_line` grouped by account_id and move_id with `sum:debit` and `sum:credit`;
 * `counted` holds the ids of the posted and reversed entries (drafts and cancelled ones do not count).
 */
export function ledgerBalances(
  groups: readonly AggGroup[],
  counted: ReadonlySet<string>,
): Map<string, number> {
  const out = new Map<string, number>();
  for (const g of groups) {
    if (!counted.has(String(g.key.move_id ?? ""))) continue;
    const account = String(g.key.account_id ?? "");
    const delta = num(g.measures["sum:debit"]) - num(g.measures["sum:credit"]);
    out.set(account, (out.get(account) ?? 0) + delta);
  }
  return out;
}
