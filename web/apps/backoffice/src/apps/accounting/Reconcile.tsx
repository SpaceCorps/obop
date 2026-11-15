// The bank reconciliation screen (`#/accounting/accounting_bank_statement_line?view=reconcile`):
// the bank transactions still to match on the left, what they can be matched to on the right, and
// one click to match. Everything it does is a command an agent can call as well (OR-4):
// - candidates: `accounting_move_line.search` (open journal items of the same amount) and
//   `bank_matching.suggest` (posted invoices of the same amount);
// - Match: `accounting_bank_statement_line.update` (the journal item) then `.match`, two calls so
//   that the state moves by itself (D48);
// - Book: `bank_matching.book` writes a draft entry, bank account against the account chosen,
//   then the transaction is matched to the entry's bank line as above;
// - Pay an open item: the transaction first records the item it settles (`settles_line_id`,
//   unique, so the server refuses a second payment of the same item), then it is booked as above.
//   An item that a transaction settles is not offered again.
// Keyboard: j and k (or the arrow keys) move between transactions, Enter runs the first proposal.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Icon, Select, fmt, getCurrency, t } from "@obop/ui";
import { call, search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { ModelPageProps } from "../../extensions.ts";
import { formatDate } from "../../format.ts";
import { useMedia } from "../../media.ts";
import { href } from "../../route.ts";
import { serverMessage } from "../../serverMessage.ts";
import { BANK_LINE, RECONCILE } from "./links.ts";
import "./accounting.css";

const MOVE = "accounting_move";
const ITEM = "accounting_move_line";
const JOURNAL = "accounting_journal";
const ACCOUNT = "accounting_account";

/** A proposal for one bank transaction. */
type Proposal =
  | { kind: "item"; item: Row; move?: Row; onBank: boolean }
  | { kind: "invoice"; invoiceId: string; number: string; reference: boolean };

type Loaded<T> =
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "ok"; value: T };

const money = (minor: unknown, currency: string) =>
  fmt.money(Number(minor) || 0, currency).replace(/^-/, "−");

const typing = (el: EventTarget | null) =>
  el instanceof HTMLElement && !!el.closest("input, select, textarea, [contenteditable]");

export function ReconcileView(p: ModelPageProps) {
  const d = useData();
  const phone = useMedia("(max-width: 56rem)");
  const [journals, setJournals] = useState<Row[] | null>(null);
  const [accounts, setAccounts] = useState<Row[]>([]);
  const [lines, setLines] = useState<Loaded<Row[]>>({ state: "loading" });
  const [selected, setSelected] = useState("");
  /** On a phone the list and the detail are two screens: whether the detail is open. */
  const [detail, setDetail] = useState(false);
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const journalId = p.route.section || "";

  // The bank and cash journals, and the accounts a transaction can be booked to.
  useEffect(() => {
    let live = true;
    void (async () => {
      const [j, a] = await Promise.all([
        search(d.token, JOURNAL, { limit: 500 }),
        search(d.token, ACCOUNT, {
          filter: [{ field: "deprecated", op: "ne", value: true }],
          order: [{ field: "code", dir: "asc" }],
          limit: 2000,
        }),
      ]);
      if (!live) return;
      setJournals(
        "error" in j
          ? []
          : j.rows.filter((r) => (r.kind === "bank" || r.kind === "cash") && r.active !== false),
      );
      setAccounts("error" in a ? [] : a.rows);
    })();
    return () => {
      live = false;
    };
  }, [d.token]);

  // The transactions still to match, oldest first (the order the bank booked them).
  useEffect(() => {
    let live = true;
    setLines({ state: "loading" });
    void (async () => {
      const r = await search(d.token, BANK_LINE, {
        filter: [
          { field: "state", op: "eq", value: "unmatched" },
          ...(journalId ? [{ field: "journal_id", op: "eq" as const, value: journalId }] : []),
        ],
        order: [{ field: "date", dir: "asc" }],
        limit: 500,
      });
      if (!live) return;
      setLines(
        "error" in r ? { state: "error", message: r.error } : { state: "ok", value: r.rows },
      );
    })();
    return () => {
      live = false;
    };
  }, [d.token, journalId, reload]);

  const rows = useMemo(() => (lines.state === "ok" ? lines.value : []), [lines]);
  // Keep a selection on a wide screen: the first transaction, or the next one after a match.
  useEffect(() => {
    if (lines.state !== "ok") return;
    if (!rows.some((r) => r.id === selected)) setSelected(phone ? "" : (rows[0]?.id ?? ""));
  }, [rows, lines.state, selected, phone]);

  const journalOf = useCallback((id: unknown) => journals?.find((j) => j.id === id), [journals]);
  const currencyOf = (line: Row | undefined) => {
    const c = journalOf(line?.journal_id)?.currency;
    return typeof c === "string" && /^[A-Z]{3}$/.test(c) ? c : getCurrency();
  };
  const line = rows.find((r) => r.id === selected);
  const index = rows.findIndex((r) => r.id === selected);
  const total = rows.reduce((n, r) => n + (Number(r.amount) || 0), 0);
  const journal = journalOf(journalId);

  const runRef = useRef<() => void>(() => {});
  // j / k / arrows move, Enter runs the first proposal; never while typing in a field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typing(e.target) || e.ctrlKey || e.metaKey || e.altKey || busy) return;
      const step =
        e.key === "j" || e.key === "ArrowDown" ? 1 : e.key === "k" || e.key === "ArrowUp" ? -1 : 0;
      if (step && rows.length) {
        e.preventDefault();
        const next = rows[Math.min(rows.length - 1, Math.max(0, index + step))];
        if (next) setSelected(next.id);
      } else if (e.key === "Enter" && line && !(e.target instanceof HTMLButtonElement)) {
        e.preventDefault();
        runRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [rows, index, line, busy]);

  const done = (message: string) => {
    d.notify(message);
    setDetail(false);
    void d.changed(BANK_LINE);
    setReload((n) => n + 1);
  };

  /** Point the transaction at `item`, then move it to matched: two calls, the state on its own. */
  async function matchTo(bankLine: Row, itemId: string): Promise<boolean> {
    const u = await call(d.token, `${BANK_LINE}.update`, { id: bankLine.id, move_line_id: itemId });
    if (u.kind !== "ok") return fail(u);
    const m = await call(d.token, `${BANK_LINE}.match`, { id: bankLine.id });
    if (m.kind !== "ok") return fail(m);
    return true;
  }
  function fail(r: Awaited<ReturnType<typeof call>>): false {
    if (r.kind === "approval")
      d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
    else if (r.kind === "error") d.notify(serverMessage(r.message), "error");
    return false;
  }

  async function match(bankLine: Row, item: Row) {
    setBusy(true);
    const ok = await matchTo(bankLine, item.id);
    setBusy(false);
    if (ok) done(t("Matched “{label}”", { label: String(bankLine.label ?? "") }));
  }

  /**
   * Book the transaction as a draft entry against `account`, then match it to the bank side. With
   * `settles`, the open item it pays is recorded first: a second payment of it is refused there.
   */
  async function book(bankLine: Row, account: string, invoice = "", settles = "") {
    setBusy(true);
    if (settles) {
      const s = await call(d.token, `${BANK_LINE}.update`, {
        id: bankLine.id,
        settles_line_id: settles,
      });
      if (s.kind !== "ok") {
        setBusy(false);
        fail(s);
        setReload((n) => n + 1);
        return;
      }
    }
    const r = await call(d.token, "bank_matching.book", {
      bank_line: bankLine.id,
      counter_account: account,
      ...(invoice ? { invoice } : {}),
    });
    if (r.kind !== "ok") {
      if (settles)
        await call(d.token, `${BANK_LINE}.update`, { id: bankLine.id, settles_line_id: null });
      setBusy(false);
      fail(r);
      return;
    }
    const moveId = String((r.value as { move_id?: string }).move_id ?? "");
    const bankAccount = journalOf(bankLine.journal_id)?.default_account_id;
    const items = await search(d.token, ITEM, {
      filter: [{ field: "move_id", op: "eq", value: moveId }],
    });
    const bankSide =
      "error" in items
        ? undefined
        : (items.rows.find((i) => i.account_id === bankAccount) ?? items.rows[0]);
    const ok = bankSide ? await matchTo(bankLine, bankSide.id) : false;
    setBusy(false);
    void d.changed(MOVE);
    if (!ok) {
      setReload((n) => n + 1);
      return;
    }
    const to = href({ app: p.app.id, model: MOVE, id: moveId });
    d.notify(t("Booked as a draft entry and matched. Post the entry when it is checked."), "ok", {
      action: { label: t("Open entry"), href: to },
    });
    setDetail(false);
    void d.changed(BANK_LINE);
    setReload((n) => n + 1);
  }

  const back = () => p.go({ ...p.route, view: "", section: "" });
  const pickJournal = (id: string) => p.go({ ...p.route, view: RECONCILE, section: id });
  const showList = !phone || !detail;
  const showDetail = !phone || detail;

  return (
    <section className="main panel acc-rec" aria-label={t("Match bank transactions")}>
      <div className="content">
        <header className="acc-rec-head">
          <div className="crumbs">
            <a href={href({ app: p.app.id, model: BANK_LINE })}>{t("Bank transactions")}</a>
            <Icon name="chevron-right" size={12} />
            <strong>{t("Match transactions")}</strong>
          </div>
          <div className="acc-rec-tools">
            {journals && journals.length > 1 && (
              <Select
                aria-label={t("Bank journal")}
                value={journalId}
                onChange={(e) => pickJournal(e.target.value)}
              >
                <option value="">{t("All bank journals")}</option>
                {journals.map((j) => (
                  <option key={j.id} value={j.id}>
                    {String(j.name ?? j.code)}
                  </option>
                ))}
              </Select>
            )}
            <Button icon="list" onClick={back}>
              {t("All transactions")}
            </Button>
          </div>
        </header>

        {lines.state === "loading" || journals === null ? (
          <div className="acc-rec-body" aria-busy="true" aria-label={t("Loading")}>
            <div className="acc-rec-list">
              {[0, 1, 2, 3].map((i) => (
                <div key={i} className="acc-rec-wait">
                  <span className="skel" />
                  <span className="skel short" />
                </div>
              ))}
            </div>
          </div>
        ) : lines.state === "error" ? (
          <p className="hint">{lines.message}</p>
        ) : rows.length === 0 ? (
          <div className="acc-rec-empty">
            <Icon name="check-circle" size={32} />
            <h2>{t("All bank transactions are matched")}</h2>
            <p className="hint">
              {journal
                ? t("Nothing is left to match in {journal}.", { journal: String(journal.name) })
                : t("Nothing is left to match. New statement lines show up here.")}
            </p>
            <Button onClick={back}>{t("Show all transactions")}</Button>
          </div>
        ) : (
          <div className={`acc-rec-body${phone ? " phone" : ""}`}>
            {showList && (
              <div className="acc-rec-side">
                <p className="acc-rec-sum hint">
                  {t("{count} to match", { count: fmt.integer(rows.length) })}
                  {(journal || new Set(rows.map((r) => currencyOf(r))).size === 1) && (
                    <span className="tabular"> · {money(total, currencyOf(rows[0]))}</span>
                  )}
                </p>
                <ul className="acc-rec-list" aria-label={t("Transactions to match")}>
                  {rows.map((r) => (
                    <li key={r.id}>
                      <button
                        type="button"
                        className="acc-rec-line"
                        aria-current={r.id === selected && !phone ? "true" : undefined}
                        onClick={() => {
                          setSelected(r.id);
                          setDetail(true);
                        }}
                      >
                        <span className="acc-rec-line-top">
                          <span className="acc-rec-date">{formatDate(r.date)}</span>
                          <strong
                            className={`tabular ${Number(r.amount) < 0 ? "acc-out" : "acc-in"}`}
                          >
                            {money(r.amount, currencyOf(r))}
                          </strong>
                        </span>
                        <span className="acc-rec-label">{String(r.label ?? "")}</span>
                        {!!r.counterparty_name && (
                          <span className="hint acc-rec-party">{String(r.counterparty_name)}</span>
                        )}
                      </button>
                    </li>
                  ))}
                </ul>
                {!phone && (
                  <p className="hint acc-rec-keys">
                    {t("Keys: j and k move, Enter takes the first proposal.")}
                  </p>
                )}
              </div>
            )}
            {showDetail && line && (
              <LineDetail
                key={line.id}
                line={line}
                currency={currencyOf(line)}
                journal={journalOf(line.journal_id)}
                accounts={accounts}
                busy={busy}
                phone={phone}
                onBack={() => setDetail(false)}
                onMatch={(item) => void match(line, item)}
                onBook={(account, invoice, settles) => void book(line, account, invoice, settles)}
                runRef={runRef}
                position={t("{n} of {count}", {
                  n: fmt.integer(index + 1),
                  count: fmt.integer(rows.length),
                })}
              />
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function LineDetail(p: {
  line: Row;
  currency: string;
  journal: Row | undefined;
  accounts: Row[];
  busy: boolean;
  phone: boolean;
  position: string;
  onBack: () => void;
  onMatch: (item: Row) => void;
  onBook: (account: string, invoice?: string, settles?: string) => void;
  runRef: React.MutableRefObject<() => void>;
}) {
  const d = useData();
  const { line } = p;
  const amount = Number(line.amount) || 0;
  const [proposals, setProposals] = useState<Loaded<Proposal[]>>({ state: "loading" });
  const accountById = useMemo(() => new Map(p.accounts.map((a) => [a.id, a])), [p.accounts]);
  const receivable = p.accounts.find((a) => a.account_type === "asset_receivable");
  const [account, setAccount] = useState("");

  useEffect(() => {
    let live = true;
    void (async () => {
      const side = amount > 0 ? "debit" : "credit";
      const [items, sug] = await Promise.all([
        search(d.token, ITEM, {
          filter: [
            { field: "reconciled", op: "ne", value: true },
            { field: side, op: "eq", value: Math.abs(amount) },
          ],
          limit: 20,
        }),
        amount > 0
          ? call(d.token, "bank_matching.suggest", { bank_line: line.id })
          : Promise.resolve(null),
      ]);
      if (!live) return;
      if ("error" in items) return setProposals({ state: "error", message: items.error });
      // Items already matched to another transaction, or paid by one, are not offered again.
      const ids = items.rows.map((i) => i.id);
      const [taken, paid] = await Promise.all([
        search(d.token, BANK_LINE, {
          filter: [{ field: "move_line_id", op: "in", value: ids }],
          limit: 50,
        }),
        search(d.token, BANK_LINE, {
          filter: [{ field: "settles_line_id", op: "in", value: ids }],
          limit: 50,
        }),
      ]);
      const used = new Set([
        ...("error" in taken ? [] : taken.rows.map((r) => String(r.move_line_id))),
        ...("error" in paid ? [] : paid.rows.map((r) => String(r.settles_line_id))),
      ]);
      const open = items.rows.filter((i) => !used.has(i.id));
      const moves = open.length
        ? await search(d.token, MOVE, {
            filter: [{ field: "id", op: "in", value: open.map((i) => i.move_id) }],
          })
        : { rows: [] as Row[] };
      if (!live) return;
      const moveOf = (id: unknown) =>
        "error" in moves ? undefined : moves.rows.find((m) => m.id === id);
      const bankAccount = p.journal?.default_account_id;
      const out: Proposal[] = open
        .map((item): Proposal => ({
          kind: "item",
          item,
          move: moveOf(item.move_id),
          onBank: !!bankAccount && item.account_id === bankAccount,
        }))
        .filter((x) => x.kind !== "item" || x.move?.state !== "cancelled")
        // A payment already booked on the bank account first, then open invoices and bills.
        .sort(
          (a, b) => Number(b.kind === "item" && b.onBank) - Number(a.kind === "item" && a.onBank),
        );
      if (sug && sug.kind === "ok") {
        const s = (sug.value as { suggestions?: { candidates?: Record<string, unknown>[] }[] })
          .suggestions?.[0];
        for (const c of s?.candidates ?? [])
          out.push({
            kind: "invoice",
            invoiceId: String(c.invoice_id),
            number: String(c.number ?? ""),
            reference: c.reference_match === true,
          });
      }
      setProposals({ state: "ok", value: out });
    })();
    return () => {
      live = false;
    };
  }, [d.token, line.id, amount, p.journal]);

  const list = proposals.state === "ok" ? proposals.value : [];
  const first = list[0];
  const accountName = (id: unknown) => {
    const a = accountById.get(String(id));
    return a ? `${String(a.code ?? "")} ${String(a.name ?? "")}`.trim() : "";
  };
  const run = (x: Proposal) => {
    if (x.kind === "item") {
      if (x.onBank) p.onMatch(x.item);
      else p.onBook(String(x.item.account_id), "", x.item.id);
    } else if (receivable) p.onBook(receivable.id, x.invoiceId);
  };
  p.runRef.current = () => {
    if (first && !p.busy) run(first);
  };

  return (
    <article className="acc-rec-detail" aria-label={String(line.label ?? "")}>
      {p.phone && (
        <Button variant="ghost" icon="chevron-left" onClick={p.onBack} className="acc-rec-back">
          {t("Back to the list")}
        </Button>
      )}
      <header className="acc-rec-card">
        <span className="hint">
          {formatDate(line.date)} · {String(p.journal?.name ?? "")} · {p.position}
        </span>
        <strong className={`acc-rec-amount tabular ${amount < 0 ? "acc-out" : "acc-in"}`}>
          {money(amount, p.currency)}
        </strong>
        <span className="acc-rec-title">{String(line.label ?? "")}</span>
        {(!!line.counterparty_name || !!line.counterparty_iban) && (
          <span className="hint">
            {[line.counterparty_name, line.counterparty_iban]
              .filter(Boolean)
              .map(String)
              .join(" · ")}
          </span>
        )}
      </header>

      <section className="acc-rec-props" aria-labelledby="acc-rec-props-h">
        <h3 id="acc-rec-props-h" className="overline">
          {t("Proposals")}
        </h3>
        {proposals.state === "loading" ? (
          <div className="acc-rec-wait">
            <span className="skel" />
            <span className="skel short" />
          </div>
        ) : proposals.state === "error" ? (
          <p className="hint">{proposals.message}</p>
        ) : list.length === 0 ? (
          <p className="hint">{t("No open item has this amount. Book it to an account below.")}</p>
        ) : (
          <ul className="acc-props">
            {list.map((x, i) => (
              <li key={x.kind === "item" ? x.item.id : x.invoiceId} className="acc-prop">
                <div className="acc-prop-text">
                  {x.kind === "item" ? (
                    <>
                      <strong>
                        {String(x.move?.number || x.move?.reference || t("Draft entry"))}
                        {x.item.label ? ` · ${String(x.item.label)}` : ""}
                      </strong>
                      <span className="hint">
                        {[
                          accountName(x.item.account_id),
                          x.move?.date ? formatDate(x.move.date) : "",
                          x.item.due_on ? t("due {date}", { date: formatDate(x.item.due_on) }) : "",
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                      <span className="hint">
                        {x.onBank
                          ? t("Already booked on the bank account: match it.")
                          : t("Open item: book the payment against it.")}
                      </span>
                      {typeof x.move?.date === "string" &&
                        typeof line.date === "string" &&
                        x.move.date > line.date && (
                          <span className="acc-prop-warn">
                            <Icon name="warning" size={12} />
                            {t("Dated after this transaction: check that it is the right one.")}
                          </span>
                        )}
                    </>
                  ) : (
                    <>
                      <strong>{t("Invoice {number}", { number: x.number })}</strong>
                      <span className="hint">
                        {x.reference
                          ? t("The amount and the invoice number match.")
                          : t("The amount matches.")}
                      </span>
                    </>
                  )}
                </div>
                <span className="acc-prop-amount tabular">{money(amount, p.currency)}</span>
                <Button
                  variant={i === 0 ? "primary" : "secondary"}
                  disabled={p.busy || (x.kind === "invoice" && !receivable)}
                  loading={p.busy && i === 0}
                  onClick={() => run(x)}
                >
                  {x.kind === "item" && x.onBank ? t("Match") : t("Book and match")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="acc-rec-book" aria-labelledby="acc-rec-book-h">
        <h3 id="acc-rec-book-h" className="overline">
          {t("Book to an account")}
        </h3>
        <form
          className="acc-book-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (account) p.onBook(account);
          }}
        >
          <Select
            label={t("Account")}
            value={account}
            onChange={(e) => setAccount(e.target.value)}
            required
          >
            <option value="">{t("Choose an account")}</option>
            {p.accounts
              .filter((a) => a.id !== p.journal?.default_account_id)
              .map((a) => (
                <option key={a.id} value={a.id}>
                  {accountName(a.id)}
                </option>
              ))}
          </Select>
          <Button type="submit" disabled={!account || p.busy}>
            {t("Book and match")}
          </Button>
        </form>
        <p className="hint">
          {t(
            "Writes a draft journal entry: the bank account against this account. Posting it stays with an approver.",
          )}
        </p>
      </section>
    </article>
  );
}
