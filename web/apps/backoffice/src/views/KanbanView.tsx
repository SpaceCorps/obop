import { useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent } from "react";
import { fmt as fmtLocale, inSentence, t, useI18n } from "@obop/ui";
import { search, type FieldMeta, type Row, type ServerFilter } from "../api.ts";
import { Cell } from "../fields.tsx";
import { labelField, useData } from "../data.tsx";
import { readPref, writePref } from "../favorites.ts";
import { isEmpty, isMasked, isState, money as fmtMoney, rowCurrency } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { serverMessage } from "../serverMessage.ts";
import {
  domainOf,
  formatSumRows,
  groupDomain,
  groupKey,
  groupLabel,
  groupable,
  sumOf,
} from "../query.ts";
import { keyOf, useAggregate } from "../useAggregate.ts";
import { AggState } from "./AggState.tsx";
import { Icon } from "../ui.tsx";
import { actionsOn, canMove, initialOption, isGoverned, moveRecord } from "../workflow.ts";
import type { ViewProps } from "./types.ts";

/** The field the board is grouped by: the person's choice, else a status, else the first groupable. */
export function kanbanField(
  fields: FieldMeta[],
  chosen: string | undefined,
): FieldMeta | undefined {
  const g = groupable(fields);
  return (
    g.find((f) => f.name === chosen) ??
    g.find((f) => isState(f)) ??
    g.find((f) => f.kind === "select") ??
    g.find((f) => f.kind === "ref") ??
    g[0]
  );
}

function defaultsFor(f: FieldMeta, key: string): Record<string, unknown> {
  if (key === "") return {};
  if (f.kind === "bool") return { [f.name]: key === "true" };
  if (f.kind === "date") return { [f.name]: `${key}-01` };
  return { [f.name]: key };
}

/** Cards a column asks for at first, and each time "Load more" is used. */
export const COLUMN_PAGE = 40;

const nextMonth = (key: string): string => {
  const [y, m] = key.split("-").map(Number) as [number, number];
  return m === 12 ? `${y + 1}-01-01` : `${y}-${String(m + 1).padStart(2, "0")}-01`;
};

/** The server conditions that select the cards of one column. */
export function columnFilter(f: FieldMeta, key: string): ServerFilter[] {
  if (key === "") return [{ field: f.name, op: "is_null", value: true }];
  if (f.kind === "bool") return [{ field: f.name, op: "eq", value: key === "true" }];
  if (f.kind === "date")
    return [
      { field: f.name, op: "gte", value: `${key}-01` },
      { field: f.name, op: "lt", value: nextMonth(key) },
    ];
  return [{ field: f.name, op: "eq", value: key }];
}

type ColumnState = { rows: Row[]; total: number; loading: boolean; error?: string };

/**
 * The cards of each column from `<model>.search`, one page per column (D4): a column asks for its
 * first cards and again for more when "Load more" raises its limit. Columns refetch after any
 * write, when the search changes and, when the cards are sorted, when the locale changes (the
 * server sorts them in the collation of the locale it is told); the cards already shown stay until
 * the new ones arrive.
 */
function useColumnCards(opts: {
  enabled: boolean;
  model: string;
  base: ServerFilter[];
  order: { field: string; dir: "asc" | "desc" }[];
  field: FieldMeta | undefined;
  keys: string[];
  folded: string[];
  limits: Record<string, number>;
}) {
  const d = useData();
  const { locale } = useI18n();
  const { enabled, model, base, order, field, keys, folded, limits } = opts;
  const [cols, setCols] = useState<Record<string, ColumnState>>({});
  const seen = useRef(new Map<string, string>());
  const seq = useRef(0);
  // Every request carries `?lang=` and `Accept-Language`, but the `lang` in the body of a search,
  // which picks the collation, is sent only with an `order` (api.ts): the locale is part of the
  // query for sorted cards and for nothing else.
  const baseSig = JSON.stringify([
    model,
    base,
    order,
    field?.name,
    d.mutations,
    order.length ? locale : null,
  ]);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    // A new search or a write starts every column over; a limit change only its own.
    seen.current.clear();
    seq.current++;
    if (!enabled) setCols({});
  }, [baseSig, enabled, attempt]);

  useEffect(() => {
    if (!enabled || !field) return;
    const token = d.token;
    const round = seq.current;
    for (const key of keys) {
      if (folded.includes(key)) continue;
      const limit = limits[key] ?? COLUMN_PAGE;
      const sig = `${round}:${limit}`;
      if (seen.current.get(key) === sig) continue;
      seen.current.set(key, sig);
      setCols((c) => ({
        ...c,
        [key]: { rows: c[key]?.rows ?? [], total: c[key]?.total ?? 0, loading: true },
      }));
      void search(token, model, {
        filter: [...base, ...columnFilter(field, key)],
        order,
        limit,
      }).then((r) => {
        if (round !== seq.current) return;
        setCols((c) => ({
          ...c,
          [key]:
            "error" in r
              ? {
                  rows: c[key]?.rows ?? [],
                  total: c[key]?.total ?? 0,
                  loading: false,
                  error: r.error,
                }
              : { rows: r.rows, total: r.total, loading: false },
        }));
      });
    }
    // baseSig stands for the search; limits and keys are listed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    enabled,
    baseSig,
    attempt,
    keys.join("\u0001"),
    folded.join("\u0001"),
    JSON.stringify(limits),
  ]);

  return { cols, retry: () => setAttempt((n) => n + 1) };
}

/** Per column: its record count and, for a money field, its sums by currency. */
export function columnTotals(
  groups: {
    key: Record<string, unknown>;
    count: number;
    measures: Record<string, number | null>;
  }[],
  field: FieldMeta,
  sumField: FieldMeta | undefined,
) {
  const out = new Map<string, { count: number; sums: Map<string, number> }>();
  for (const g of groups) {
    const k = keyOf(g.key[field.name]);
    const e = out.get(k) ?? { count: 0, sums: new Map<string, number>() };
    e.count += g.count;
    if (sumField) {
      const c = typeof g.key.currency === "string" ? g.key.currency : "";
      e.sums.set(c, (e.sums.get(c) ?? 0) + (g.measures[`sum:${sumField.name}`] ?? 0));
    }
    out.set(k, e);
  }
  return out;
}

export function KanbanView(p: ViewProps) {
  const d = useData();
  const { fields, model } = p;
  const gf = kanbanField(fields, p.state.group[0]);
  const server = p.server;
  const money1 = fields.find((f) => f.kind === "money");
  const foldKey = `obop-fold:${model.name}:${gf?.name}`;
  const [folded, setFolded] = useState<string[]>(() => readPref<string[]>(foldKey, []));
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [quick, setQuick] = useState<{ key: string; text: string } | null>(null);
  const board = useRef<HTMLDivElement>(null);
  const pendingFocus = useRef<{ id: string; col: number } | null>(null);
  useEffect(() => {
    const want = pendingFocus.current;
    if (!want) return;
    const el = board.current?.querySelector<HTMLElement>(
      `[data-col="${want.col}"] [data-card="${want.id}"]`,
    );
    if (el) {
      pendingFocus.current = null;
      el.focus();
    }
  });
  const [limits, setLimits] = useState<Record<string, number>>({});

  // Server mode (D4): counts and sums per column from one aggregate, cards one page per column.
  const hasCurrency = model.fields.some(
    (f) => f.name === "currency" && (f.kind === "text" || f.kind === "select"),
  );
  const agg = useAggregate({
    enabled: !!server && !!gf,
    model: model.name,
    filter: server?.filter ?? [],
    groupBy: gf
      ? [
          gf.kind === "date" ? { field: gf.name, bucket: "month" } : { field: gf.name },
          ...(hasCurrency && money1 ? [{ field: "currency" }] : []),
        ]
      : [],
    measures: money1 ? [{ op: "count" }, { op: "sum", field: money1.name }] : [{ op: "count" }],
  });
  const totals = useMemo(
    () => (gf && server ? columnTotals(agg.groups, gf, money1) : null),
    [gf, server, agg.groups, money1],
  );

  const clientRows = p.rows;
  const keys = useMemo(() => {
    if (!gf) return [];
    if (!server) return groupDomain(gf, clientRows, d.lookup);
    const present = [...(totals?.keys() ?? [])];
    return domainOf(gf, present, d.lookup);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [gf, server, clientRows, totals, d.version, d.lang]);
  const { cols, retry: retryColumns } = useColumnCards({
    enabled: !!server && !!gf && agg.loaded && !agg.error,
    model: model.name,
    base: server?.filter ?? [],
    order: server?.order ?? [],
    field: gf,
    keys,
    folded,
    limits,
  });
  /** Every card on screen: the loaded pages (server) or the records (browser). */
  const rows = useMemo(
    () => (server ? keys.flatMap((k) => cols[k]?.rows ?? []) : clientRows),
    [server, keys, cols, clientRows],
  );
  if (!gf) return null;
  if (server && (agg.error || !agg.loaded))
    return (
      <AggState agg={agg}>
        <></>
      </AggState>
    );

  const titleField = labelField(model);
  const secondary = fields
    .filter((f) => f.name !== gf.name && f.name !== titleField?.name)
    .sort((a, b) => weight(a) - weight(b))
    .slice(0, 3);
  // A governed board (D48) moves cards only along declared actions the person may run.
  const governed = isGoverned(model, gf);
  const canDrag =
    gf.kind !== "date" && (governed ? actionsOn(model, gf.name).length > 0 : p.canUpdate);
  const noun = p.title;
  /** Whether the card being dragged (or the given one) may go to column `key`. */
  const may = (id: string | null, key: string) => {
    if (!governed || !id) return { ok: true, reason: "" };
    const row = rows.find((r) => r.id === id);
    if (!row) return { ok: true, reason: "" };
    const m = canMove(model, gf, row[gf.name] as string, key, noun);
    return { ok: m.allowed || m.current, reason: m.reason };
  };

  const setFold = (key: string) => {
    const next = folded.includes(key) ? folded.filter((k) => k !== key) : [...folded, key];
    setFolded(next);
    writePref(foldKey, next);
  };

  async function move(id: string, key: string) {
    if (!gf) return;
    const row = rows.find((r) => r.id === id);
    if (!row || groupKey(gf, row) === key) return;
    if (key === "" && gf.required)
      return d.notify(t("{field} is required", { field: fieldTitle(gf) }), "error");
    if (governed) {
      const m = may(id, key);
      if (!m.ok) return d.notify(m.reason, "error");
      const out = await moveRecord(d.run, model, gf, row, key, noun);
      if (out.kind === "approval") d.notify(out.message, "info");
      else if (out.kind === "error") d.notify(out.message, "error");
      return;
    }
    const value = key === "" ? null : gf.kind === "bool" ? key === "true" : key;
    const err = await p.update(id, { [gf.name]: value });
    if (err) d.notify(err, "error");
  }
  const drop = (e: DragEvent, key: string) => {
    e.preventDefault();
    setOver(null);
    const id = e.dataTransfer.getData("text/plain");
    if (id) void move(id, key);
  };

  async function submitQuick() {
    if (!quick || !gf || !titleField) return;
    const text = quick.text.trim();
    if (!text) return setQuick(null);
    const err = await p.create({ [titleField.name]: text, ...defaultsFor(gf, quick.key) });
    if (err) d.notify(err, "error");
    else setQuick(null);
  }

  const cardKey = (e: KeyboardEvent, row: Row, key: string, index: number) => {
    const focus = (c: number, i: number) => {
      const cards = board.current?.querySelectorAll<HTMLElement>(`[data-col="${c}"] [data-card]`);
      cards?.[Math.min(i, cards.length - 1)]?.focus();
    };
    const col = keys.indexOf(key);
    if (e.key === "Enter") {
      e.preventDefault();
      p.onOpen(row.id);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      focus(col, index + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      focus(col, Math.max(0, index - 1));
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const to = col + (e.key === "ArrowRight" ? 1 : -1);
      if (to < 0 || to >= keys.length) return;
      if (e.shiftKey && canDrag) {
        // The card reappears in its new column once the board has asked again: focus it then.
        pendingFocus.current = { id: row.id, col: to };
        setTimeout(() => (pendingFocus.current = null), 5000);
        void move(row.id, keys[to]!);
      } else focus(to, index);
    }
  };

  return (
    <div
      className="kanban"
      ref={board}
      role="group"
      aria-label={t("{model} board by {field}", { model: p.title, field: fieldTitle(gf) })}
    >
      {keys.map((key, col) => {
        const cards = server
          ? (cols[key]?.rows ?? [])
          : rows.filter((r) => groupKey(gf, r) === key);
        const count = server ? (totals?.get(key)?.count ?? 0) : cards.length;
        const sums = totals?.get(key)?.sums;
        const total = money1
          ? server
            ? [...(sums?.values() ?? [])].reduce((a, b) => a + b, 0)
            : sumOf(cards, money1.name)
          : null;
        const sumText =
          server && money1 && sums
            ? fmtLocale.list(
                [...sums].map(([c, n]) => fmtMoney(n, c || undefined)),
                "conjunction",
              )
            : formatSumRows(money1, cards);
        const more = server ? count - cards.length : 0;
        const isFolded = folded.includes(key);
        const name = groupLabel(gf, key, d.lookup);
        const lines = secondary;
        return (
          <section
            key={key || "none"}
            data-col={col}
            className={`kcol${over === key ? " over" : ""}${isFolded ? " folded" : ""}${dragging && !may(dragging, key).ok ? " blocked" : ""}`}
            title={dragging && !may(dragging, key).ok ? may(dragging, key).reason : undefined}
            aria-label={t("{name}, {count, plural, one {# card} other {# cards}}", {
              name,
              count,
            })}
            onDragOver={(e) => {
              if (!canDrag) return;
              if (!may(dragging, key).ok) {
                e.dataTransfer.dropEffect = "none";
                return;
              }
              e.preventDefault();
              setOver(key);
            }}
            onDragLeave={() => setOver((o) => (o === key ? null : o))}
            onDrop={(e) => canDrag && drop(e, key)}
          >
            <header>
              <button
                className="foldbtn"
                aria-expanded={!isFolded}
                aria-label={isFolded ? t("Unfold {name}", { name }) : t("Fold {name}", { name })}
                onClick={() => setFold(key)}
              >
                <Icon name={isFolded ? "chevron-right" : "chevron-down"} size={13} />
              </button>
              <b className="kname">
                {gf.kind === "select" ? (
                  key ? (
                    <Cell field={gf} value={key} />
                  ) : (
                    <span className="badge t-neutral">{t("None")}</span>
                  )
                ) : (
                  name
                )}
              </b>
              <span className="count">{count}</span>
              {!isFolded && total !== null && total > 0 && (
                <span className="sum" title={t("Total {field}", { field: fieldTitle(money1!) })}>
                  {sumText}
                </span>
              )}
              {p.canCreate && !isFolded && (!governed || key === initialOption(gf)) && (
                <button
                  className="icon-btn"
                  aria-label={t("Add a card to {name}", { name })}
                  onClick={() => setQuick({ key, text: "" })}
                >
                  <Icon name="plus" size={14} />
                </button>
              )}
            </header>
            {!isFolded && quick?.key === key && (
              <form
                className="quick"
                onSubmit={(e) => {
                  e.preventDefault();
                  void submitQuick();
                }}
              >
                {titleField ? (
                  <input
                    autoFocus
                    aria-label={t("New {field}", { field: inSentence(fieldTitle(titleField)) })}
                    placeholder={t("{field}…", { field: fieldTitle(titleField) })}
                    value={quick.text}
                    onChange={(e) => setQuick({ key, text: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.stopPropagation();
                        setQuick(null);
                      }
                    }}
                  />
                ) : (
                  <span className="hint">{t("Open the full form to fill this card.")}</span>
                )}
                <div className="actionrow">
                  {titleField && (
                    <button type="submit" className="btn small primary">
                      {t("Add")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      setQuick(null);
                      p.newWith({
                        ...defaultsFor(gf, key),
                        ...(titleField && quick.text.trim()
                          ? { [titleField.name]: quick.text.trim() }
                          : {}),
                      });
                    }}
                  >
                    {t("Edit")}
                  </button>
                </div>
              </form>
            )}
            {!isFolded &&
              cards.map((r, i) => (
                <article
                  key={r.id}
                  data-card={r.id}
                  tabIndex={0}
                  aria-label={d.recordLabel(model.name, r)}
                  className={`kcard${r.id === p.selectedId ? " sel" : ""}${dragging === r.id ? " drag" : ""}`}
                  draggable={canDrag}
                  onDragStart={(e) => {
                    e.dataTransfer.setData("text/plain", r.id);
                    e.dataTransfer.effectAllowed = "move";
                    setDragging(r.id);
                  }}
                  onDragEnd={() => setDragging(null)}
                  onClick={() => p.onOpen(r.id)}
                  onKeyDown={(e) => cardKey(e, r, key, i)}
                >
                  <strong className="ktitle">{d.recordLabel(model.name, r)}</strong>
                  {lines.map((f) =>
                    isEmpty(r[f.name]) ? null : (
                      <div key={f.name} className={`kline${f.kind === "money" ? " strong" : ""}`}>
                        <span>{fieldTitle(f)}</span>
                        <span className={isMasked(r[f.name]) ? "" : "kval"}>
                          <Cell
                            field={f}
                            value={r[f.name]}
                            link={false}
                            currency={rowCurrency(r)}
                            row={r}
                          />
                        </span>
                      </div>
                    ),
                  )}
                  {/* Keyboard and touch twin of dragging */}
                  {canDrag && (
                    <select
                      className="moveto"
                      aria-label={t("Move {name} to", { name: d.recordLabel(model.name, r) })}
                      value=""
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => e.stopPropagation()}
                      onChange={(e) =>
                        e.target.value !== "" &&
                        void move(r.id, e.target.value === "\u0000" ? "" : e.target.value)
                      }
                    >
                      <option value="">{t("Move to…")}</option>
                      {keys
                        .filter((k) => k !== key && (k !== "" || !gf.required))
                        .map((k) => {
                          const m = governed ? canMove(model, gf, key, k, noun) : null;
                          return (
                            <option
                              key={k}
                              value={k === "" ? "\u0000" : k}
                              disabled={m ? !m.allowed : false}
                              title={m && !m.allowed ? m.reason : undefined}
                            >
                              {groupLabel(gf, k, d.lookup)}
                              {m?.allowed && m.needsApproval ? " *" : ""}
                            </option>
                          );
                        })}
                    </select>
                  )}
                </article>
              ))}
            {!isFolded && more > 0 && (
              <button
                className="btn small kmore"
                disabled={cols[key]?.loading}
                onClick={() =>
                  setLimits((l) => ({ ...l, [key]: (l[key] ?? COLUMN_PAGE) + COLUMN_PAGE }))
                }
              >
                {cols[key]?.loading
                  ? t("Loading…")
                  : t("Load more ({count} left)", { count: more })}
              </button>
            )}
            {!isFolded && cols[key]?.error && (
              <p className="kempty" role="alert">
                {serverMessage(cols[key]!.error)}{" "}
                <button className="linkbtn" onClick={retryColumns}>
                  {t("Try again")}
                </button>
              </p>
            )}
            {!isFolded && count === 0 && cards.length === 0 && quick?.key !== key && (
              <p className="kempty">
                {canDrag ? t("Nothing here. Drop a card.") : t("Nothing here.")}
              </p>
            )}
          </section>
        );
      })}
    </div>
  );
}

function weight(f: FieldMeta): number {
  return {
    image: -1,
    money: 0,
    decimal: 0,
    select: 1,
    date: 2,
    datetime: 2,
    ref: 3,
    number: 4,
    bool: 5,
    text: 6,
    file: 7,
    longtext: 9,
  }[f.kind];
}
