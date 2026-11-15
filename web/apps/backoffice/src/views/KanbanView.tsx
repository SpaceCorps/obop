import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent,
  type TouchEvent,
} from "react";
import {
  fmt as fmtLocale,
  getCurrency,
  inSentence,
  panelId,
  t,
  tabId,
  Tabs,
  useI18n,
  type MenuItem,
} from "@obop/ui";
import { search, type FieldMeta, type Row, type ServerFilter } from "../api.ts";
import { Cell, FieldInput } from "../fields.tsx";
import { useData } from "../data.tsx";
import { fromEdit, toEdit, type Edit } from "../edit.ts";
import { readPref, writePref } from "../favorites.ts";
import { isEmpty, isState, money as fmtMoney } from "../format.ts";
import { actionTitle, fieldTitle } from "../labels.ts";
import { PHONE_QUERY, useMedia } from "../media.ts";
import { newDefaults, nextActions } from "../presentation.ts";
import { domainOf, groupDomain, groupKey, groupLabel, groupable } from "../query.ts";
import { serverMessage } from "../serverMessage.ts";
import { Icon } from "../ui.tsx";
import { useAggregate } from "../useAggregate.ts";
import {
  actionsOn,
  canMove,
  governedNames,
  initialOption,
  isGoverned,
  moveRecord,
  notForRecord,
} from "../workflow.ts";
import { AggState } from "./AggState.tsx";
import { BoardCard, cardTitle, usePresentCtx, type NextStep } from "./BoardCard.tsx";
import {
  boardLayout,
  columnTotals,
  draggableBy,
  FOLD_ABOVE,
  isFolded,
  isTerminal,
  mainMoneyOf,
  quickPlan,
  readFold,
  rowSums,
  showsSum,
  toggleFold,
  type FoldPrefs,
} from "./board.ts";
import type { ViewProps } from "./types.ts";

export { columnTotals } from "./board.ts";

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
  // Every command carries `?lang=` and `Accept-Language` (api.ts `send`). In the body, `search()`
  // adds the `lang` that picks the collation only with an `order`, so the locale is part of the
  // query for sorted cards and for nothing else. (`aggregate()`, which gives the counts of the
  // columns, always adds it; that is not asked again either, see the docs on the language.)
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

/** The quick form of a column: what was typed, and why it cannot be saved yet. */
type Quick = {
  key: string;
  values: Record<string, Edit>;
  errors: Record<string, string>;
  error?: string;
  busy?: boolean;
};

/** A card just made with the quick form: shown first in its column until the search changes. */
type Fresh = { key: string; row: Row; at: number };

/** The sentence of a column's sums ("€12,400.00 and SEK 3,000.00"); negative sums show too. */
const sumSentence = (sums: Map<string, number>) =>
  fmtLocale.list(
    [...sums].filter(([, n]) => n !== 0).map(([c, n]) => fmtMoney(n, c || undefined)),
    "conjunction",
  );

const SWIPE_PX = 56;

/**
 * A board (D4): one column per value of the grouping, counts and sums from one aggregate, cards one
 * page per column. Cards follow the card layout; a ⋯ menu (a long press on a touch screen) moves a
 * card, and a governed board (D48) puts the next step of each card on a small button. Cards are
 * dragged only between the values of a select or a yes/no, never between refs (B9). A finished
 * column with many cards starts folded. On a phone the board shows one column at a time behind a
 * strip of stages with their counts; a swipe goes to the next stage.
 */
export function KanbanView(p: ViewProps) {
  const d = useData();
  const { fields, model } = p;
  const gf = kanbanField(fields, p.state.group[0]);
  const server = p.server;
  const phone = useMedia(PHONE_QUERY);
  const ctx = usePresentCtx(d.models.get(model.name)?.module.id);
  const money1 = useMemo(() => mainMoneyOf(model, fields), [model, fields]);
  const layout = useMemo(() => boardLayout(model, fields, gf), [model, fields, gf]);
  const foldKey = `obop-fold:${model.name}:${gf?.name}`;
  const [fold, setFoldPrefs] = useState<FoldPrefs>(() => readFold(readPref<unknown>(foldKey, [])));
  useEffect(() => setFoldPrefs(readFold(readPref<unknown>(foldKey, []))), [foldKey]);
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [quick, setQuick] = useState<Quick | null>(null);
  const [fresh, setFresh] = useState<Fresh | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [stage, setStage] = useState<string | null>(null);
  const board = useRef<HTMLDivElement>(null);
  const pendingFocus = useRef<{ id: string; col: number } | null>(null);
  const swipe = useRef<{ x: number; y: number } | null>(null);
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
  // A new search drops the card made a moment ago from the top of its column.
  const searchSig = JSON.stringify([model.name, server?.filter, gf?.name, p.state.facets]);
  useEffect(() => setFresh(null), [searchSig]);
  // The new card scrolls into view once it is drawn.
  useEffect(() => {
    if (!fresh) return;
    board.current
      ?.querySelector<HTMLElement>(`[data-card="${fresh.row.id}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [fresh]);

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
    () => (gf && server ? columnTotals(agg.groups, gf, money1, getCurrency()) : null),
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
  const countOf = (key: string) =>
    server
      ? (totals?.get(key)?.count ?? 0)
      : gf
        ? clientRows.filter((r) => groupKey(gf, r) === key).length
        : 0;
  const finished = (key: string) => !!gf && isTerminal(model, gf, key);
  const foldedKeys = phone
    ? []
    : keys.filter((k) => isFolded(fold, k, finished(k) && countOf(k) > FOLD_ABOVE));
  // On a phone one stage shows: the chosen one, else the first that has cards.
  const active =
    stage !== null && keys.includes(stage)
      ? stage
      : (keys.find((k) => countOf(k) > 0 && !finished(k)) ?? keys[0] ?? "");
  const { cols, retry: retryColumns } = useColumnCards({
    enabled: !!server && !!gf && agg.loaded && !agg.error,
    model: model.name,
    base: server?.filter ?? [],
    order: server?.order ?? [],
    field: gf,
    keys,
    folded: phone ? keys.filter((k) => k !== active) : foldedKeys,
    limits,
  });
  /** Every card on screen: the loaded pages (server) or the records (browser). */
  const rows = useMemo(() => {
    const all = server ? keys.flatMap((k) => cols[k]?.rows ?? []) : clientRows;
    return fresh && !all.some((r) => r.id === fresh.row.id) ? [fresh.row, ...all] : all;
  }, [server, keys, cols, clientRows, fresh]);
  if (!gf) return null;
  if (server && (agg.error || !agg.loaded))
    return (
      <AggState agg={agg}>
        <></>
      </AggState>
    );

  // A governed board (D48) moves cards only along declared actions the person may run.
  const governed = isGoverned(model, gf);
  const canDrag =
    draggableBy(gf) && (governed ? actionsOn(model, gf.name).length > 0 : p.canUpdate);
  const noun = p.title;
  const titleOfRow = (r: Row) => cardTitle(model, r, layout, ctx);
  /** Whether the card being dragged (or the given one) may go to column `key`. */
  const may = (id: string | null, key: string) => {
    if (!governed || !id) return { ok: true, reason: "" };
    const row = rows.find((r) => r.id === id);
    if (!row) return { ok: true, reason: "" };
    // The card itself decides between two actions that lead the same way, or refuses one (D92).
    const m = canMove(model, gf, row[gf.name] as string, key, noun, row);
    return { ok: m.allowed || m.current, reason: m.reason };
  };

  const setFold = (key: string) => {
    const next = toggleFold(fold, key, finished(key) && countOf(key) > FOLD_ABOVE);
    setFoldPrefs(next);
    writePref(foldKey, next);
  };

  async function move(id: string, key: string) {
    if (!gf || !canDrag) return;
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

  /** The next step of a card on a governed board: its primary action, as the record panel has it. */
  const nextOf = (r: Row): NextStep | undefined => {
    if (!governed) return undefined;
    const a = nextActions(model, r).primary;
    if (!a || notForRecord(model, a, r)) return undefined;
    const name = actionTitle(a);
    return {
      label: name,
      approval: a.needs_approval,
      busy: busyId === r.id,
      run: () => {
        // An action that needs input the card does not have opens the record (D95 `needs`).
        if (a.ui?.needs?.some((n) => isEmpty(r[n]))) return p.onOpen(r.id);
        void (async () => {
          if (a.ui?.confirm && !(await d.confirm(a.ui.confirm, name))) return;
          setBusyId(r.id);
          if (a.field === gf.name && a.to) {
            const out = await moveRecord(d.run, model, gf, r, a.to, noun);
            setBusyId(null);
            if (out.kind === "ok") d.notify(a.ui?.done ?? t("{name} done", { name }));
            else d.notify(out.message, out.kind === "approval" ? "info" : "error");
            return;
          }
          const reply = await d.run(a.id, { id: r.id });
          setBusyId(null);
          if (reply.kind === "ok") d.notify(a.ui?.done ?? t("{name} done", { name }));
          else if (reply.kind === "approval")
            d.notify(
              t(
                "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
                { name, plan: reply.planId.slice(0, 8) },
              ),
              "info",
            );
          else d.notify(serverMessage(reply.message), "error");
        })();
      },
    };
  };

  /** The ⋯ menu of a card: every other column, the ones it cannot go to greyed out with why. */
  const menuOf = (r: Row, key: string): MenuItem[] => {
    if (!canDrag) return [];
    const targets = keys.filter((k) => k !== key && (k !== "" || !gf.required));
    if (!targets.length) return [];
    return [
      { heading: t("Move to…") },
      ...targets.map((k): MenuItem => {
        const m = governed ? canMove(model, gf, key, k, noun, r) : null;
        return {
          label: `${groupLabel(gf, k, d.lookup)}${m?.allowed && m.needsApproval ? " *" : ""}`,
          disabled: m ? !m.allowed : false,
          description: m && !m.allowed ? m.reason : undefined,
          onSelect: () => void move(r.id, k),
        };
      }),
    ];
  };

  // ---- quick create ----

  const governedSet = governedNames(model);
  const presetFor = (key: string) =>
    newDefaults(model, {
      facets: p.state.facets,
      route: defaultsFor(gf, key),
      instanceCurrency: getCurrency(),
    });
  const planFor = (key: string) => quickPlan(model, fields, presetFor(key));
  const startQuick = (key: string) => {
    const plan = planFor(key);
    if (plan.full) return p.newWith(presetFor(key));
    const values: Record<string, Edit> = {};
    const preset = presetFor(key);
    for (const f of plan.fields) values[f.name] = toEdit(f, preset[f.name]);
    setQuick({ key, values, errors: {} });
  };
  const quickTitle = (key: string) => planFor(key).fields.find((f) => f.kind === "text");

  async function submitQuick() {
    if (!quick || !gf) return;
    const plan = planFor(quick.key);
    const preset = presetFor(quick.key);
    const title = plan.fields.find((f) => f.kind === "text");
    // Nothing typed at all: the form closes, as Escape would.
    if (plan.fields.every((f) => isEmpty(quick.values[f.name]) || quick.values[f.name] === false))
      return setQuick(null);
    const body: Record<string, unknown> = { ...preset };
    const errors: Record<string, string> = {};
    for (const f of plan.fields) {
      const r = fromEdit(f, quick.values[f.name] ?? "", getCurrency());
      if ("error" in r) errors[f.name] = r.error;
      else if (r.value !== null) body[f.name] = r.value;
    }
    if (Object.keys(errors).length) return setQuick({ ...quick, errors, error: undefined });
    for (const f of model.fields)
      if (f.computed || f.sequence || governedSet.has(f.name)) delete body[f.name];
    setQuick({ ...quick, busy: true, errors: {}, error: undefined });
    const r = await d.run(`${model.name}.create`, body);
    if (r.kind === "ok") {
      const row = r.value as Row;
      if (row?.id) setFresh({ key: quick.key, row, at: Date.now() });
      // The form stays for the next card, emptied but for what the column fills in.
      const values: Record<string, Edit> = {};
      for (const f of plan.fields)
        values[f.name] = f === title ? "" : (quick.values[f.name] ?? toEdit(f, preset[f.name]));
      setQuick({ key: quick.key, values, errors: {} });
      return;
    }
    if (r.kind === "approval") {
      d.notify(
        t(
          "Approval needed: creating the record was proposed as plan {plan}. Nothing changes until a human approves it.",
          { plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
      return setQuick(null);
    }
    // The server's words, made a sentence for a person (B10); shown in the form, not only a toast.
    setQuick({ ...quick, busy: false, error: serverMessage(r.message) });
  }

  // ---- keyboard ----

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
    } else if ((e.key === "ArrowRight" || e.key === "ArrowLeft") && !phone) {
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

  // ---- one column ----

  const column = (key: string, col: number, solo: boolean) => {
    const loaded = server
      ? (cols[key]?.rows ?? [])
      : clientRows.filter((r) => groupKey(gf, r) === key);
    const isFresh = fresh?.key === key;
    const cards =
      isFresh && fresh ? [fresh.row, ...loaded.filter((r) => r.id !== fresh.row.id)] : loaded;
    const count = server ? (totals?.get(key)?.count ?? 0) : loaded.length;
    const sums = server ? totals?.get(key)?.sums : rowSums(loaded, money1);
    const more = server ? count - loaded.length : 0;
    const isFolded = !solo && foldedKeys.includes(key);
    const name = groupLabel(gf, key, d.lookup);
    const done = finished(key);
    const blocked = !!dragging && !may(dragging, key).ok;
    const canAdd = p.canCreate && !isFolded && (!governed || key === initialOption(gf));
    const titleField = quickTitle(key);
    return (
      <section
        key={key || "none"}
        data-col={col}
        id={solo ? panelId("kstage") : undefined}
        role={solo ? "tabpanel" : "region"}
        aria-labelledby={solo ? tabId("kstage", key || "none") : undefined}
        className={`kcol${over === key ? " over" : ""}${isFolded ? " folded" : ""}${blocked ? " blocked" : ""}${solo ? " solo" : ""}`}
        title={blocked ? may(dragging, key).reason : undefined}
        aria-label={
          solo
            ? undefined
            : t("{name}, {count, plural, one {# card} other {# cards}}", { name, count })
        }
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
        onTouchStart={
          solo
            ? (e: TouchEvent) => {
                const t0 = e.touches[0];
                swipe.current = t0 ? { x: t0.clientX, y: t0.clientY } : null;
              }
            : undefined
        }
        onTouchEnd={
          solo
            ? (e: TouchEvent) => {
                const s = swipe.current;
                const t1 = e.changedTouches[0];
                swipe.current = null;
                if (!s || !t1) return;
                const dx = t1.clientX - s.x;
                const dy = t1.clientY - s.y;
                if (Math.abs(dx) < SWIPE_PX || Math.abs(dx) < Math.abs(dy) * 1.5) return;
                const rtl = getComputedStyle(e.currentTarget).direction === "rtl";
                const step = dx < 0 !== rtl ? 1 : -1;
                const to = keys[col + step];
                if (to !== undefined) setStage(to);
              }
            : undefined
        }
      >
        <header>
          <div className="khead-row">
            {!solo && (
              <button
                className="foldbtn"
                aria-expanded={!isFolded}
                aria-label={isFolded ? t("Unfold {name}", { name }) : t("Fold {name}", { name })}
                onClick={() => setFold(key)}
              >
                <Icon name={isFolded ? "chevron-right" : "chevron-down"} size={13} />
              </button>
            )}
            {/* On a phone the strip of stages above names the stage and counts it. */}
            {!solo && (
              <>
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
              </>
            )}
            {solo && !isFolded && money1 && showsSum(sums) && (
              <span className="sum" title={t("Total {field}", { field: fieldTitle(money1) })}>
                {sumSentence(sums!)}
              </span>
            )}
            <span className="grow" />
            {canAdd &&
              (solo ? (
                <button
                  className="btn small kadd"
                  aria-label={t("Add a card to {name}", { name })}
                  onClick={() => startQuick(key)}
                >
                  <Icon name="plus" size={14} />
                  {t("Add")}
                </button>
              ) : (
                <button
                  className="icon-btn"
                  aria-label={t("Add a card to {name}", { name })}
                  onClick={() => startQuick(key)}
                >
                  <Icon name="plus" size={14} />
                </button>
              ))}
          </div>
          {!solo && !isFolded && money1 && showsSum(sums) && (
            <span className="sum" title={t("Total {field}", { field: fieldTitle(money1) })}>
              {sumSentence(sums!)}
            </span>
          )}
        </header>
        {!isFolded && quick?.key === key && (
          <form
            className="quick"
            noValidate
            onSubmit={(e) => {
              e.preventDefault();
              void submitQuick();
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                setQuick(null);
              }
            }}
          >
            {planFor(key).fields.map((f, i) => {
              const err = quick.errors[f.name];
              const errId = `kq-${model.name}-${f.name}-err`;
              const set = (v: Edit) =>
                setQuick((q) =>
                  q
                    ? {
                        ...q,
                        values: { ...q.values, [f.name]: v },
                        errors: { ...q.errors, [f.name]: "" },
                      }
                    : q,
                );
              if (f === titleField)
                return (
                  <div key={f.name} className="qfield">
                    <input
                      autoFocus
                      aria-label={t("New {field}", { field: inSentence(fieldTitle(f)) })}
                      aria-invalid={err ? true : undefined}
                      aria-describedby={err ? errId : undefined}
                      aria-required={f.required || undefined}
                      placeholder={t("{field}…", { field: fieldTitle(f) })}
                      value={String(quick.values[f.name] ?? "")}
                      onChange={(e) => set(e.target.value)}
                    />
                    {err && (
                      <small className="error" id={errId}>
                        {err}
                      </small>
                    )}
                  </div>
                );
              const id = `kq-${model.name}-${f.name}`;
              return (
                <div key={f.name} className="qfield">
                  <label htmlFor={id}>
                    {fieldTitle(f)}
                    {f.required && <span aria-hidden="true"> *</span>}
                  </label>
                  <FieldInput
                    field={f}
                    id={id}
                    compact
                    autoFocus={i === 0}
                    value={quick.values[f.name] ?? toEdit(f, undefined)}
                    invalid={!!err}
                    describedBy={err ? errId : undefined}
                    currency={getCurrency()}
                    onChange={set}
                  />
                  {err && (
                    <small className="error" id={errId}>
                      {err}
                    </small>
                  )}
                </div>
              );
            })}
            {quick.error && (
              <p className="error banner" role="alert">
                {quick.error}
              </p>
            )}
            <div className="actionrow">
              <button type="submit" className="btn small primary" disabled={quick.busy}>
                {t("Add")}
              </button>
              <button
                type="button"
                className="btn small"
                onClick={() => {
                  const preset = presetFor(key);
                  const typed: Record<string, unknown> = {};
                  for (const f of planFor(key).fields) {
                    const r = fromEdit(f, quick.values[f.name] ?? "", getCurrency());
                    if ("value" in r && r.value !== null) typed[f.name] = r.value;
                  }
                  setQuick(null);
                  p.newWith({ ...preset, ...typed });
                }}
              >
                {t("Edit")}
              </button>
              <span className="grow" />
              <button
                type="button"
                className="icon-btn"
                aria-label={t("Close")}
                onClick={() => setQuick(null)}
              >
                <Icon name="close" size={14} />
              </button>
            </div>
          </form>
        )}
        {!isFolded &&
          cards.map((r, i) => {
            const title = titleOfRow(r);
            return (
              <BoardCard
                key={r.id}
                model={model}
                fields={fields}
                row={r}
                layout={layout}
                ctx={ctx}
                title={title}
                selected={r.id === p.selectedId}
                fresh={isFresh && fresh?.row.id === r.id}
                finished={done}
                draggable={canDrag && !phone}
                dragging={dragging === r.id}
                menu={menuOf(r, key)}
                menuLabel={t("Move {name} to", { name: title })}
                next={nextOf(r)}
                onOpen={() => p.onOpen(r.id)}
                onKeyDown={(e) => cardKey(e, r, key, i)}
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/plain", r.id);
                  e.dataTransfer.effectAllowed = "move";
                  setDragging(r.id);
                }}
                onDragEnd={() => setDragging(null)}
              />
            );
          })}
        {!isFolded && more > 0 && (
          <button
            className="btn small kmore"
            disabled={cols[key]?.loading}
            onClick={() =>
              setLimits((l) => ({ ...l, [key]: (l[key] ?? COLUMN_PAGE) + COLUMN_PAGE }))
            }
          >
            {cols[key]?.loading ? t("Loading…") : t("Load more ({count} left)", { count: more })}
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
            {canDrag && !phone ? t("Nothing here. Drop a card.") : t("Nothing here.")}
          </p>
        )}
      </section>
    );
  };

  const label = t("{model} board by {field}", { model: p.title, field: fieldTitle(gf) });

  if (phone)
    return (
      <div className="kanban phone" ref={board} role="group" aria-label={label}>
        <div className="kstages">
          <Tabs
            label={fieldTitle(gf)}
            idPrefix="kstage"
            value={active || "none"}
            onChange={(v) => setStage(v === "none" ? "" : v)}
            tabs={keys.map((k) => ({
              id: k || "none",
              label: groupLabel(gf, k, d.lookup),
              count: countOf(k),
            }))}
          />
        </div>
        {column(active, keys.indexOf(active), true)}
      </div>
    );

  return (
    <div className="kanban" ref={board} role="group" aria-label={label}>
      {keys.map((key, col) => column(key, col, false))}
    </div>
  );
}
