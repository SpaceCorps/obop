// The home of Accounting (`#/accounting/overview`): one card per journal, as in Odoo's accounting
// dashboard. A bank or cash journal shows its statement balance and what is left to reconcile, the
// others the drafts waiting to be posted. Every figure is `<model>.search` with `aggregate` (D4),
// the call an agent makes, and every link opens the list or screen behind the figure.
import { useEffect, useMemo, useState } from "react";
import { Button, Icon, fmt, getCurrency, t } from "@obop/ui";
import { LOAD_LIMIT, aggregate, search, type ModelMeta } from "../../api.ts";
import { AppIcon, appBlurb, type App } from "../../Chrome.tsx";
import { useData } from "../../data.tsx";
import { optionLabel } from "../../format.ts";
import { quickFilters } from "../../filters.ts";
import { today } from "../../quickFilters.ts";
import { appMenu, pluralTitle } from "../../nav.ts";
import { encodeState, type Cond } from "../../query.ts";
import { href } from "../../route.ts";
import { hasStatements, journalCards, ledgerBalances, type JournalCard } from "./journals.ts";
import { reconcileHref } from "./links.ts";
import "./accounting.css";

const MOVE = "accounting_move";
const LINE = "accounting_move_line";
const BANK = "accounting_bank_statement_line";
const JOURNAL = "accounting_journal";

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "ready"; cards: JournalCard[]; overdue: number | null };

/** Money on one line that never wraps inside the amount. */
const money = (minor: number, currency: string) =>
  fmt.money(minor, currency).replace(/^-/, "−").replace(/\s/g, " ");

/** The ledger balance of the bank and cash accounts: posted and reversed entries only. */
async function bankLedger(token: string, accounts: string[]): Promise<Map<string, number>> {
  if (!accounts.length) return new Map();
  const g = await aggregate(token, LINE, {
    filter: [{ field: "account_id", op: "in", value: accounts }],
    group_by: [{ field: "account_id" }, { field: "move_id" }],
    measures: [
      { op: "sum", field: "debit" },
      { op: "sum", field: "credit" },
    ],
  });
  if ("error" in g) return new Map();
  const ids = [...new Set(g.groups.map((x) => String(x.key.move_id ?? "")))].filter(Boolean);
  const posted = ids.length
    ? await search(token, MOVE, {
        filter: [
          { field: "id", op: "in", value: ids },
          { field: "state", op: "in", value: ["posted", "reversed"] },
        ],
        limit: LOAD_LIMIT,
      })
    : { rows: [] };
  if ("error" in posted) return new Map();
  return ledgerBalances(g.groups, new Set(posted.rows.map((r) => r.id)));
}

export function AccountingOverview({ app }: { app: App }) {
  const d = useData();
  const model = (name: string): ModelMeta | undefined =>
    app.module.models.find((m) => m.name === name);
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let live = true;
    void (async () => {
      const [journals, moves, bank, overdue] = await Promise.all([
        search(d.token, JOURNAL, { limit: 500 }),
        aggregate(d.token, MOVE, {
          group_by: [{ field: "journal_id" }, { field: "state" }],
          measures: [{ op: "count" }, { op: "sum", field: "total_debit" }],
        }),
        aggregate(d.token, BANK, {
          group_by: [{ field: "journal_id" }, { field: "state" }],
          measures: [{ op: "count" }, { op: "sum", field: "amount" }],
        }),
        aggregate(d.token, LINE, {
          filter: [
            { field: "reconciled", op: "eq", value: false },
            { field: "due_on", op: "lt", value: today() },
          ],
        }),
      ]);
      if (!live) return;
      if ("error" in journals) return setState({ kind: "error", message: journals.error });
      const ledger = await bankLedger(
        d.token,
        journals.rows
          .filter((j) => j.kind === "bank" || j.kind === "cash")
          .map((j) => String(j.default_account_id ?? ""))
          .filter(Boolean),
      );
      if (!live) return;
      const cards = journalCards(
        journals.rows,
        "error" in moves ? [] : moves.groups,
        "error" in bank ? [] : bank.groups,
        getCurrency(),
        ledger,
      );
      setState({ kind: "ready", cards, overdue: "error" in overdue ? null : overdue.total });
    })();
    return () => {
      live = false;
    };
  }, [d.token, d.mutations]);

  const kindField = model(JOURNAL)?.fields.find((f) => f.name === "kind");
  const toMatch = state.kind === "ready" && state.cards.some((c) => c.toMatch.count > 0);
  const moveModel = model(MOVE);
  const menu = useMemo(() => appMenu(app.module), [app.module]);
  const listOf = (name: string, facets: Cond[], extra: { id?: string } = {}) =>
    href({ app: app.id, model: name, st: encodeState({ facets, group: [] }), ...extra });
  // The journal items' own "Overdue" filter (due_on < today && reconciled = false), as the count.
  const lineModel = model(LINE);
  const overdueFilter =
    lineModel &&
    quickFilters(lineModel, lineModel.fields, { user: "", today: today() }).find(
      (q) =>
        q.kind === "saved" &&
        (lineModel.ui?.filter?.[Number(q.key.slice(6))]?.where ?? "").startsWith("due_on < today "),
    );
  const overdueHref = overdueFilter
    ? listOf(LINE, overdueFilter.conds)
    : href({ app: app.id, model: LINE });

  return (
    <section
      className="main panel overview acc-overview"
      aria-label={t("{app} overview", { app: app.name })}
    >
      <div className="content">
        <header className="ov-head">
          <AppIcon app={app} size={48} />
          <div className="ov-title">
            <h1>{app.name}</h1>
            <p className="hint">{appBlurb(app)}</p>
          </div>
          {moveModel?.can_create && (
            <div className="ov-actions">
              <Button
                // One primary action per screen: while a bank card asks to match, that is the one.
                variant={toMatch ? "secondary" : "primary"}
                icon="plus"
                onClick={() => (location.hash = href({ app: app.id, model: MOVE, id: "new" }))}
              >
                {t("New journal entry")}
              </Button>
            </div>
          )}
        </header>

        <section className="ov-section" aria-labelledby="acc-journals">
          <h2 id="acc-journals" className="overline">
            {t("Journals")}
          </h2>
          {state.kind === "loading" ? (
            <div className="acc-cards" aria-busy="true" aria-label={t("Loading")}>
              {[0, 1, 2].map((i) => (
                <div key={i} className="acc-card acc-card-wait">
                  <span className="skel" />
                  <span className="skel short" />
                </div>
              ))}
            </div>
          ) : state.kind === "error" ? (
            <p className="hint">{state.message}</p>
          ) : state.cards.length === 0 ? (
            <p className="ov-calm">
              <Icon name="info" size={18} />
              {t("No journal yet. Add one under Configuration, Journals.")}
            </p>
          ) : (
            <ul className="acc-cards">
              {state.cards.map((c) => (
                <li key={c.id}>
                  <JournalCardView
                    card={c}
                    kindLabel={optionLabel(c.kind, kindField)}
                    draftsHref={listOf(MOVE, [
                      { op: "is", f: "journal_id", v: [c.id] },
                      { op: "is", f: "state", v: ["draft"] },
                    ])}
                    postedHref={listOf(MOVE, [
                      { op: "is", f: "journal_id", v: [c.id] },
                      { op: "is", f: "state", v: ["posted"] },
                    ])}
                    statementsHref={listOf(BANK, [{ op: "is", f: "journal_id", v: [c.id] }])}
                    reconcileHref={reconcileHref(app.id, c.id)}
                    newHref={
                      moveModel?.can_create
                        ? listOf(MOVE, [{ op: "is", f: "journal_id", v: [c.id] }], { id: "new" })
                        : undefined
                    }
                  />
                </li>
              ))}
            </ul>
          )}
        </section>

        {state.kind === "ready" && state.overdue !== null && (
          <section className="ov-section" aria-labelledby="acc-follow">
            <h2 id="acc-follow" className="overline">
              {t("Follow-up")}
            </h2>
            <a className="acc-follow" href={overdueHref}>
              <Icon name="clock" size={18} />
              <span>
                {state.overdue
                  ? t("{count} overdue journal items", { count: fmt.integer(state.overdue) })
                  : t("No overdue journal items")}
              </span>
              <Icon name="chevron-right" size={14} />
            </a>
          </section>
        )}

        <section className="ov-section" aria-labelledby="acc-lists">
          <h2 id="acc-lists" className="overline">
            {t("Records")}
          </h2>
          <ul className="ov-lists">
            {[...menu.main, ...menu.reporting].map((m) => (
              <li key={m.name}>
                <a href={href({ app: app.id, model: m.name })}>
                  <span>{pluralTitle(m, app.module.id)}</span>
                  <Icon name="chevron-right" size={14} />
                </a>
              </li>
            ))}
          </ul>
          {menu.configuration.length > 0 && (
            <p className="ov-config">
              <span className="hint">{t("Configuration")}</span>
              {menu.configuration.map((m) => (
                <a key={m.name} href={href({ app: app.id, model: m.name })}>
                  {pluralTitle(m, app.module.id)}
                </a>
              ))}
            </p>
          )}
        </section>
      </div>
    </section>
  );
}

function JournalCardView(p: {
  card: JournalCard;
  kindLabel: string;
  draftsHref: string;
  postedHref: string;
  statementsHref: string;
  reconcileHref: string;
  newHref?: string;
}) {
  const c = p.card;
  const bank = hasStatements(c);
  return (
    <article className={`acc-card acc-${c.kind}`} aria-labelledby={`acc-j-${c.id}`}>
      <header className="acc-card-head">
        <div className="acc-card-name">
          <h3 id={`acc-j-${c.id}`}>{c.name}</h3>
          <span className="hint">
            {c.code} · {p.kindLabel}
          </span>
        </div>
        <span className="acc-cur">{c.currency}</span>
      </header>

      {bank ? (
        <div className="acc-figure">
          <span className="hint">{t("Balance in the ledger")}</span>
          <strong className="tabular">{money(c.ledgerBalance, c.currency)}</strong>
        </div>
      ) : (
        <div className="acc-figure">
          <span className="hint">{t("Posted")}</span>
          <strong className="tabular">{money(c.posted.sum, c.currency)}</strong>
        </div>
      )}

      <dl className="acc-rows">
        {bank && (
          <a href={c.toMatch.count ? p.reconcileHref : p.statementsHref} className="acc-row">
            <dt>{t("To match")}</dt>
            <dd>
              <span className={c.toMatch.count ? "acc-count warn" : "acc-count"}>
                {fmt.integer(c.toMatch.count)}
              </span>
              {c.toMatch.count > 0 && (
                <span className="tabular">{money(c.toMatch.sum, c.currency)}</span>
              )}
            </dd>
          </a>
        )}
        <a href={p.draftsHref} className="acc-row">
          <dt>{t("Drafts to post")}</dt>
          <dd>
            <span className={c.drafts.count ? "acc-count info" : "acc-count"}>
              {fmt.integer(c.drafts.count)}
            </span>
            {c.drafts.count > 0 && (
              <span className="tabular">{money(c.drafts.sum, c.currency)}</span>
            )}
          </dd>
        </a>
        {!bank && (
          <a href={p.postedHref} className="acc-row">
            <dt>{t("Posted entries")}</dt>
            <dd>
              <span className="acc-count">{fmt.integer(c.posted.count)}</span>
            </dd>
          </a>
        )}
      </dl>

      <footer className="acc-card-foot">
        {bank ? (
          c.toMatch.count > 0 ? (
            <a className="btn primary" href={p.reconcileHref}>
              <Icon name="check" size={16} />
              {t("Match transactions ({count})", { count: fmt.integer(c.toMatch.count) })}
            </a>
          ) : (
            <span className="acc-done">
              <Icon name="check-circle" size={16} />
              {c.statementLines ? t("All matched") : t("No statement lines yet")}
            </span>
          )
        ) : (
          p.newHref && (
            <a className="btn" href={p.newHref}>
              <Icon name="plus" size={16} />
              {t("New entry")}
            </a>
          )
        )}
      </footer>
    </article>
  );
}
