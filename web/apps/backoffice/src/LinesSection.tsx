// The lines of a document inside its form (lane L7 of the UI plan): for each child of the model
// (presentation.ts `childrenOf`), a section in the parent's form. Three shapes (lines.ts `shapeOf`):
//
//   lines   a table where every cell is edited in place (order, invoice, journal entry, BOM, POS
//           lines). Tab and Shift+Tab move, Enter saves the row (and on the last row adds one),
//           Escape reverts it, the bin deletes it. Picking a product fills the description and
//           the price (`fill_from`). The footer adds up the amounts; a journal entry shows the
//           difference between debit and credit. Too narrow for the columns (a phone), it becomes
//           one card per line with an edit sheet.
//   list    a compact list of records with a workflow of their own (a vehicle's services,
//           translations): title, state and date, "Add", "Open all (n)" past 50.
//   images  a grid of thumbnails, reordered by dragging (or Alt+arrow keys).
//
// Every change is the child's own `create`, `update` or `delete` command, so agents keep parity
// (OR-4). On a parent that is not saved yet, the first "Add a line" saves it first. After each
// write the parent is read again, so its computed totals follow.
import {
  useEffect,
  useId,
  useMemo,
  useReducer,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { fmt, getCurrency, t } from "@obop/ui";
import type { ChildMeta, FieldMeta, ModelMeta, Row, ServerOrder } from "./api.ts";
import { BlobThumb } from "./BlobFields.tsx";
import { useData } from "./data.tsx";
import { fromEdit, toEdit, type Edit } from "./edit.ts";
import { Cell, FieldInput } from "./fields.tsx";
import type { LinesSlotProps } from "./FormBody.tsx";
import { isEmpty, optionLabel } from "./format.ts";
import { fieldTitle, modelTitle } from "./labels.ts";
import {
  ACTIONS_WIDTH,
  POSITION,
  differenceOf,
  editableField,
  fillsFor,
  fitColumns,
  lineColumns,
  lineDefaults,
  lineFrozen,
  lineOrder,
  linesFrozen,
  minWidth,
  numeric,
  shapeOf,
  sumOf,
  summedColumns,
  type LineShape,
} from "./lines.ts";
import { PHONE_QUERY, useMedia } from "./media.ts";
import {
  childrenOf,
  currencyOf,
  newDefaults,
  stateFieldOf,
  titleFieldsOf,
  titleOf,
  type MetaIndex,
  type PresentCtx,
} from "./presentation.ts";
import { encodeState } from "./query.ts";
import { serverMessage } from "./serverMessage.ts";
import { Badge, Button, Dialog, Icon, IconButton } from "./ui.tsx";
import { useServerRows } from "./useServer.ts";
import "./lines.css";

/** How many lines the table loads; past it, "Open all" leads to the list. */
const TABLE_LIMIT = 200;
/** How many records a compact list shows. */
const LIST_LIMIT = 50;

/**
 * A line to open once its parent exists: the first "Add a line" on a new record saves the record,
 * which mounts a new panel for it; the new panel's section opens the line. Keyed by
 * `<parent model>:<parent id>`, the value is the child model.
 */
const pendingAdd = new Map<string, string>();

export function LinesSection(p: LinesSlotProps) {
  const d = useData();
  const index: MetaIndex = useMemo(() => {
    const m = new Map<string, ModelMeta>();
    for (const [name, info] of d.models) m.set(name, info.model);
    return m;
  }, [d.models]);
  const kids = childrenOf(p.model, index);
  if (kids.length === 0) return null;
  return (
    <>
      {kids.map((c) => {
        const model = index.get(c.model);
        if (!model) return null;
        return (
          <ChildSection
            key={c.model}
            slot={p}
            child={c}
            model={model}
            moduleId={d.models.get(c.model)?.module.id ?? ""}
            shape={shapeOf(p.model, c, model)}
            index={index}
          />
        );
      })}
    </>
  );
}

type SectionProps = {
  slot: LinesSlotProps;
  child: ChildMeta;
  model: ModelMeta;
  moduleId: string;
  shape: LineShape;
  index: MetaIndex;
};

/** What the section keeps between renders; written in place and drawn with `render()`. */
type Lines = {
  /** Edits by row key (a saved row's id, or `new-<n>`), as form values. */
  drafts: Map<string, Record<string, Edit>>;
  /** New rows not saved yet, in order. */
  fresh: string[];
  /** Fields a pick filled in, by row: a later pick may replace them, typing keeps them. */
  auto: Map<string, Set<string>>;
  /** Fields that failed the check, by row. */
  invalid: Map<string, Set<string>>;
  /** The server's words, by row. */
  errors: Map<string, string>;
  saving: Set<string>;
  /** Rows made here that the search has not returned yet. */
  created: Row[];
  /** Rows deleted here that the search still returns. */
  deleted: Set<string>;
  seq: number;
  /** New rows the person has typed in; an untouched new row is dropped when left. */
  touched: Set<string>;
  /** The row (and column) that takes the focus after the next render. */
  focus: { key: string; col?: number; select?: boolean } | null;
};

const freshKey = (k: string) => k.startsWith("new-");
const APPROVAL =
  "Approval needed: the change was proposed (plan {plan}). A human must approve it first.";

function ChildSection(props: SectionProps) {
  const { slot, child, model, moduleId, shape, index } = props;
  const d = useData();
  const via = child.via;
  const parent = slot.model;
  const parentId = slot.row?.id ?? "";
  const phone = useMedia(PHONE_QUERY);
  const headId = useId();
  const box = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(0);
  const [, render] = useReducer((x: number) => x + 1, 0);
  const s = useRef<Lines>({
    drafts: new Map(),
    fresh: [],
    auto: new Map(),
    invalid: new Map(),
    errors: new Map(),
    saving: new Set(),
    created: [],
    deleted: new Set(),
    seq: 0,
    touched: new Set(),
    focus: null,
  }).current;
  const [sheet, setSheet] = useState<string | null>(null);

  // The rows of this parent, from the server (asked again after every write, as every page is).
  const limit = shape === "list" || shape === "translations" ? LIST_LIMIT : TABLE_LIMIT;
  const srv = useChildRows(model.name, via, parentId, limit, lineOrder(model));
  const denied = new Set(srv.denied.map((x) => (x.includes(".") ? x.split(".")[1]! : x)));
  const fields = model.fields.filter((f) => !denied.has(f.name));

  // The width decides the columns; a phone always gets cards.
  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver !== "function") return;
    const ro = new ResizeObserver(([e]) => setWidth(Math.round(e!.contentRect.width)));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Rows made here leave `created` once the search returns them, and deleted ones leave `deleted`.
  useEffect(() => {
    const ids = new Set(srv.rows.map((r) => r.id));
    const before = s.created.length + s.deleted.size;
    s.created = s.created.filter((r) => !ids.has(r.id));
    for (const id of s.deleted) if (!ids.has(id)) s.deleted.delete(id);
    if (s.created.length + s.deleted.size !== before) render();
  }, [srv.rows, s]);

  const ctx: PresentCtx = {
    refText: d.refText,
    rowOf: (m, id) => d.store.peek(m)?.find((r) => r.id === id),
    modelOf: (m) => index.get(m),
    instanceCurrency: getCurrency(),
    moduleId,
  };
  const currency = currencyOf(slot.row ?? undefined, parent, ctx) ?? getCurrency();

  // Read from `s` each time, so a handler that runs after a save sees the row it made.
  const savedNow = (): Row[] =>
    [...srv.rows, ...s.created.filter((c) => !srv.rows.some((r) => r.id === c.id))].filter(
      (r) => !s.deleted.has(r.id),
    );
  const saved = savedNow();
  const byId = new Map(saved.map((r) => [r.id, r]));
  const keys = [...saved.map((r) => r.id), ...s.fresh];

  const lock = linesFrozen(parent, slot.row, model, via);
  const creatingParent = slot.row === null;
  const canAdd =
    model.can_create && !model.via_action && !lock.frozen && (!creatingParent || slot.canEdit);
  const rowEditable = (key: string) =>
    !lock.frozen &&
    (freshKey(key)
      ? model.can_create
      : model.can_update && !lineFrozen(model, byId.get(key) ?? null));
  const rowDeletable = (key: string) =>
    freshKey(key) ||
    (model.can_delete && !lock.frozen && !lineFrozen(model, byId.get(key) ?? null));
  const edits = fields.filter((f) => editableField(model, f, via));
  const fieldOf = (name: string) => fields.find((f) => f.name === name);

  const base = (key: string, f: FieldMeta): Edit => {
    const r = byId.get(key);
    return r ? toEdit(f, r[f.name], currency) : f.kind === "bool" ? false : "";
  };
  const val = (key: string, f: FieldMeta): Edit => s.drafts.get(key)?.[f.name] ?? base(key, f);
  const changedFields = (key: string) => {
    const dr = s.drafts.get(key);
    return dr ? edits.filter((f) => f.name in dr && dr[f.name] !== base(key, f)) : [];
  };
  const dirty = (key: string) => freshKey(key) || changedFields(key).length > 0;

  /** The rows as stored values with the drafts applied, for sums and the balance. */
  const live = (): Record<string, unknown>[] => {
    const now = savedNow();
    const rowOf = new Map(now.map((r) => [r.id, r]));
    return [...now.map((r) => r.id), ...s.fresh].map((key) => {
      const out: Record<string, unknown> = { ...rowOf.get(key) };
      const dr = s.drafts.get(key);
      if (dr)
        for (const name of Object.keys(dr)) {
          const f = fieldOf(name);
          if (!f) continue;
          const r = fromEdit(f, dr[name]!, currency);
          if ("value" in r) out[name] = r.value;
        }
      return out;
    });
  };

  function setValue(key: string, f: FieldMeta, v: Edit) {
    const draft = { ...s.drafts.get(key), [f.name]: v };
    s.touched.add(key);
    s.auto.get(key)?.delete(f.name);
    s.invalid.get(key)?.delete(f.name);
    s.errors.delete(key);
    if (f.kind === "ref" && f.to && typeof v === "string") {
      const picked = v ? d.store.peek(f.to)?.find((r) => r.id === v) : undefined;
      const fills = fillsFor({
        child: model,
        ref: f,
        target: index.get(f.to),
        picked,
        label: picked ? d.recordLabel(f.to, picked) : "",
      });
      const auto = s.auto.get(key) ?? new Set<string>();
      for (const [name, value] of Object.entries(fills)) {
        const g = fieldOf(name);
        if (!g || !editableField(model, g, via)) continue;
        const now = name in draft ? draft[name]! : base(key, g);
        if (now === "" || now === false || auto.has(name)) {
          draft[name] = toEdit(g, value, currency);
          auto.add(name);
        }
      }
      s.auto.set(key, auto);
      // In the table, a pick moves on to the next cell of the row still empty (a journal line's
      // debit), its text selected; a row the pick completed keeps the focus, for Enter.
      if (v && !cards) {
        const at = fit.shown.indexOf(f);
        const next = fit.shown.findIndex((g, i) => {
          if (i <= at || !editableField(model, g, via)) return false;
          const now = g.name in draft ? draft[g.name]! : base(key, g);
          return (
            now === "" ||
            ((g.kind === "money" || g.kind === "number") && numeric(fromEditNumber(g, now)) === 0)
          );
        });
        if (next > at) s.focus = { key, col: next, select: true };
      }
    }
    s.drafts.set(key, draft);
    render();
  }

  /** A typed amount as a number (0 when it is not one yet). */
  function fromEditNumber(f: FieldMeta, e: Edit): unknown {
    const r = fromEdit(f, e, currency);
    return "value" in r ? r.value : 0;
  }

  function revert(key: string) {
    s.drafts.delete(key);
    s.auto.delete(key);
    s.invalid.delete(key);
    s.errors.delete(key);
    s.touched.delete(key);
    if (freshKey(key)) s.fresh = s.fresh.filter((k) => k !== key);
    render();
  }

  const cols = lineColumns(model, via, fields);
  const fit = fitColumns(cols, width || 640);
  const cardsAt = (w: number) =>
    shape === "lines" && (phone || (w > 0 && !fitColumns(cols, w).fits));
  const cards = cardsAt(width);

  /** Start a new line; on a parent that is not saved yet, save it first. */
  async function add(focus = true) {
    if (!canAdd) return;
    if (creatingParent) {
      const id = await slot.save();
      if (id) pendingAdd.set(`${parent.name}:${id}`, model.name);
      return;
    }
    const key = `new-${++s.seq}`;
    const rows = live();
    const defaults: Record<string, unknown> = {
      ...newDefaults(model, { instanceCurrency: getCurrency() }),
      ...lineDefaults({ child: model, via, parent, parentRow: slot.row, parentId, siblings: rows }),
    };
    const pos = edits.find((f) => f.kind === "number" && POSITION.test(f.name));
    if (pos) defaults[pos.name] = Math.max(0, ...rows.map((r) => numeric(r[pos.name]))) + 1;
    const draft: Record<string, Edit> = {};
    for (const [name, v] of Object.entries(defaults)) {
      const f = fieldOf(name);
      if (f && editableField(model, f, via)) draft[name] = toEdit(f, v, currency);
    }
    s.drafts.set(key, draft);
    s.fresh = [...s.fresh, key];
    // Measured now: on a panel that has just opened the observer may not have answered yet.
    const w = Math.round(box.current?.getBoundingClientRect().width ?? width);
    if (shape !== "lines" || cardsAt(w)) setSheet(key);
    else if (focus) s.focus = { key };
    render();
  }

  // The line asked for before the parent was saved opens now.
  useEffect(() => {
    if (!parentId) return;
    const k = `${parent.name}:${parentId}`;
    if (pendingAdd.get(k) === model.name) {
      pendingAdd.delete(k);
      void add();
    }
    // Only when the parent changes: `add` reads the current rows itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parentId]);

  /**
   * Save a row: `update` with its changed fields, or `create`. True when it is saved (or had
   * nothing to save). `explicit` (Enter, the sheet's Save) marks what is missing; leaving an
   * incomplete new row keeps it as a draft and marks only what is wrong, not what is empty.
   */
  async function commit(key: string, explicit: boolean): Promise<boolean> {
    if (s.saving.has(key) || !parentId) return false;
    const isNew = freshKey(key);
    if (!isNew && changedFields(key).length === 0) return true;
    const check = isNew ? edits : changedFields(key);
    const bad = new Set<string>();
    const body: Record<string, unknown> = {};
    let firstError = "";
    for (const f of check) {
      const r = fromEdit(f, val(key, f), currency);
      if ("error" in r) {
        bad.add(f.name);
        firstError ||= r.error;
      } else if (!isNew || r.value !== null) body[f.name] = r.value;
    }
    if (bad.size) {
      if (explicit || !isNew) {
        s.invalid.set(key, bad);
        d.notify(firstError, "error");
      } else {
        const wrong = [...bad].filter((n) => {
          const f = fieldOf(n);
          return !!f && String(val(key, f)).trim() !== "";
        });
        s.invalid.set(key, new Set(wrong));
      }
      render();
      return false;
    }
    s.saving.add(key);
    s.invalid.delete(key);
    render();
    const sent = { ...s.drafts.get(key) };
    const r = isNew
      ? await d.run(`${model.name}.create`, { [via]: parentId, ...body })
      : await d.run(`${model.name}.update`, { id: key, ...body });
    s.saving.delete(key);
    if (r.kind === "ok") {
      if (isNew) {
        s.created = [...s.created, r.value as Row];
        s.fresh = s.fresh.filter((k) => k !== key);
        s.drafts.delete(key);
      } else {
        // Keep what was typed while the save ran.
        const now = s.drafts.get(key) ?? {};
        const left = Object.fromEntries(Object.entries(now).filter(([n, v]) => sent[n] !== v));
        if (Object.keys(left).length) s.drafts.set(key, left);
        else s.drafts.delete(key);
      }
      s.auto.delete(key);
      s.errors.delete(key);
      render();
      // The parent's totals are computed from the lines: read it again.
      void d.changed(parent.name);
      return true;
    }
    if (r.kind === "approval") {
      d.notify(t(APPROVAL, { plan: r.planId.slice(0, 8) }), "info");
      revert(key);
      return true;
    }
    s.errors.set(key, r.message);
    d.notify(serverMessage(r.message), "error");
    render();
    return false;
  }

  async function remove(key: string) {
    if (freshKey(key)) return revert(key);
    if (s.saving.has(key)) return;
    s.saving.add(key);
    render();
    const r = await d.run(`${model.name}.delete`, { id: key });
    s.saving.delete(key);
    if (r.kind === "ok") {
      s.deleted.add(key);
      s.drafts.delete(key);
      s.errors.delete(key);
      void d.changed(parent.name);
    } else if (r.kind === "approval") {
      d.notify(t(APPROVAL, { plan: r.planId.slice(0, 8) }), "info");
    } else {
      s.errors.set(key, r.message);
      d.notify(serverMessage(r.message), "error");
    }
    render();
  }

  // The focus goes to the first cell of a new row (or the same cell of the next row) once it is
  // drawn, after the panel took the focus on opening.
  useEffect(() => {
    const want = s.focus;
    if (!want) return;
    // The row may be drawn only by the next render (a line opened by an effect): wait for it.
    const row = box.current?.querySelector(`[data-line="${want.key}"]`);
    if (!row) return;
    s.focus = null;
    requestAnimationFrame(() => {
      const cells = [...row.querySelectorAll("td")];
      const scope = (want.col !== undefined && want.col >= 0 ? cells[want.col] : undefined) ?? row;
      const el = scope.querySelector<HTMLElement>(
        "input:not([readonly]):not([type=hidden]), select:not(:disabled), textarea:not([readonly])",
      );
      el?.focus();
      if (want.select && el instanceof HTMLInputElement) el.select();
    });
  });

  // ---- the heading, the lock and the footer, shared by every shape ----
  const heading =
    shape === "images"
      ? t("Images")
      : shape === "translations"
        ? t("Translations")
        : /_lines?$/.test(model.name)
          ? t("Lines")
          : modelTitle(model, moduleId);
  const rows = live();
  const diff = differenceOf(model, rows);
  const count = Math.max(0, srv.total + s.created.length - s.deleted.size);
  const openAll =
    parentId && srv.total > limit
      ? d.linkTo(model.name, {
          st: encodeState({ facets: [{ op: "is", f: via, v: [parentId] }], group: [] }),
        })
      : null;
  const head = (
    <header className="lines-head">
      <h3 id={headId}>{heading}</h3>
      {count > 0 && <span className="lines-count">{fmt.number(count)}</span>}
      {diff !== null && keys.length > 0 && (
        <span className={`lines-diff ${diff === 0 ? "ok" : "off"}`} role="status">
          {diff === 0
            ? t("Balanced")
            : t("Difference {amount}", { amount: fmt.money(Math.abs(diff), currency) })}
        </span>
      )}
      <span className="grow" />
      {lock.frozen && (
        <span className="lines-lock">
          <Icon name="lock" size={13} /> {t("Locked")}
        </span>
      )}
    </header>
  );
  const lockNote = lock.frozen && (
    <p className="hint lines-locknote">
      {lock.message ??
        t("These lines cannot be changed in this state of the {model}.", {
          model: modelTitle(parent, d.models.get(parent.name)?.module.id ?? "").toLowerCase(),
        })}
    </p>
  );
  const foot = (extra?: ReactNode) => (
    <footer className="lines-foot">
      {canAdd && (
        <Button
          variant="ghost"
          size="sm"
          icon="plus"
          disabled={slot.busy}
          onClick={() => void add()}
        >
          {shape === "images" ? t("Add an image") : shape === "lines" ? t("Add a line") : t("Add")}
        </Button>
      )}
      {openAll && (
        <a className="reflink small" href={openAll}>
          {t("Open all ({count})", { count: fmt.number(srv.total) })}
        </a>
      )}
      <span className="grow" />
      {extra}
    </footer>
  );
  /**
   * What a saved line is called in its sheet: what its first column shows (the product, the
   * account) or its description, else its title without the parent (an order's line is not
   * "SO-00007"), else "Line n".
   */
  const lineTitle = (key: string): string => {
    const r = byId.get(key);
    if (!r) return t("Line {n}", { n: keys.indexOf(key) + 1 });
    for (const f of cols.filter((c) => c.kind === "ref" || c.kind === "text")) {
      const v = r[f.name];
      if (isEmpty(v)) continue;
      return f.kind === "ref" && f.to ? d.refText(f.to, String(v)) : String(v);
    }
    const own = titleOf(model, { ...r, [via]: undefined } as Row, ctx);
    return own || t("Line {n}", { n: keys.indexOf(key) + 1 });
  };
  const sheetKey = sheet && (s.drafts.has(sheet) || byId.has(sheet)) ? sheet : null;
  const sheetEl = sheetKey && (
    <LineSheet
      key={sheetKey}
      title={
        freshKey(sheetKey)
          ? shape === "images"
            ? t("New image")
            : t("New line")
          : lineTitle(sheetKey)
      }
      fields={fields.filter(
        (f) =>
          f.name !== via &&
          // An image's place is set by dragging it, not typed.
          !(shape === "images" && POSITION.test(f.name)) &&
          !(freshKey(sheetKey) && (f.computed || f.sequence)),
      )}
      model={model}
      via={via}
      row={byId.get(sheetKey) ?? null}
      value={(f) => val(sheetKey, f)}
      invalid={s.invalid.get(sheetKey)}
      error={s.errors.get(sheetKey)}
      editable={rowEditable(sheetKey)}
      deletable={rowDeletable(sheetKey) && !freshKey(sheetKey)}
      busy={s.saving.has(sheetKey)}
      currency={currency}
      onChange={(f, v) => setValue(sheetKey, f, v)}
      onSave={async () => {
        if (await commit(sheetKey, true)) setSheet(null);
      }}
      onDelete={async () => {
        await remove(sheetKey);
        setSheet(null);
      }}
      onClose={() => {
        if (dirty(sheetKey)) revert(sheetKey);
        setSheet(null);
      }}
    />
  );

  if (shape === "images")
    return (
      <section className="lines lines-images" ref={box} aria-labelledby={headId}>
        {head}
        {lockNote}
        <ImageGrid
          model={model}
          rows={saved}
          canMove={!lock.frozen && model.can_update}
          canDelete={rowDeletable}
          busy={(id) => s.saving.has(id)}
          onOpen={setSheet}
          onDelete={(id) => void remove(id)}
          canAdd={canAdd}
          onAdd={() => void add()}
          title={(r) => lineTitle(r.id)}
        />
        {saved.length === 0 && !canAdd && <p className="lines-empty">{t("No images yet")}</p>}
        {sheetEl}
      </section>
    );

  if (shape === "list" || shape === "translations")
    return (
      <section className="lines lines-list" ref={box} aria-labelledby={headId}>
        {head}
        {lockNote}
        <CompactList
          model={model}
          via={via}
          rows={saved}
          cols={cols}
          currency={currency}
          ctx={ctx}
        />
        {saved.length === 0 && <p className="lines-empty">{t("Nothing here yet")}</p>}
        {foot()}
        {sheetEl}
      </section>
    );

  // ---- lines: a table, or cards on a phone ----
  const summed = summedColumns(model, cols);
  const numberCol = (f: FieldMeta) =>
    f.kind === "money" || f.kind === "decimal" || f.kind === "number";
  const sums = summed.map((f) => ({
    f,
    text:
      f.kind === "money"
        ? fmt.money(sumOf(rows, f.name), currency)
        : fmt.number(sumOf(rows, f.name)),
  }));
  const sheetOnly = fields.filter(
    (f) => f.name !== via && !fit.shown.includes(f) && (editableField(model, f, via) || f.computed),
  );
  const empty = keys.length === 0;
  const lineNo = (key: string) => keys.indexOf(key) + 1;

  const onKey = (key: string, e: KeyboardEvent<HTMLElement>) => {
    const tgt = e.target as HTMLElement;
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      if (tgt.tagName === "BUTTON" || tgt.tagName === "TEXTAREA" || tgt.tagName === "A") return;
      e.preventDefault();
      const tr = tgt.closest("tr");
      const td = tgt.closest("td");
      const col = tr && td ? [...tr.children].indexOf(td) : undefined;
      const at = keys.indexOf(key);
      void (async () => {
        if (!(await commit(key, true))) return;
        // The next row keeps the column; after the last one a new line opens.
        const next = keys.slice(at + 1).find((k) => k !== key && keys.includes(k));
        if (next && (!freshKey(next) || s.fresh.includes(next))) {
          s.focus = { key: next, col };
          render();
        } else void add();
      })();
    } else if (e.key === "Escape" && dirty(key)) {
      e.preventDefault();
      e.stopPropagation();
      revert(key);
    }
  };

  return (
    <section
      className={`lines lines-doc${cards ? " is-cards" : ""}`}
      ref={box}
      aria-labelledby={headId}
    >
      {head}
      {lockNote}
      {cards
        ? keys.length > 0 && (
            <ul className="lines-cards" aria-labelledby={headId}>
              {keys.map((key, i) => (
                <LineCard
                  key={key}
                  cols={cols}
                  values={rows[i]!}
                  fresh={freshKey(key)}
                  currency={currency}
                  bad={!!s.errors.get(key) || !!s.invalid.get(key)?.size}
                  busy={s.saving.has(key)}
                  no={i + 1}
                  onOpen={() => setSheet(key)}
                />
              ))}
            </ul>
          )
        : !empty && (
            <table className="lines-table" aria-labelledby={headId}>
              <colgroup>
                {fit.shown.map((f) => (
                  <col
                    key={f.name}
                    style={
                      f.kind === "ref" || f.kind === "text"
                        ? undefined
                        : { inlineSize: minWidth(f) }
                    }
                  />
                ))}
                <col style={{ inlineSize: ACTIONS_WIDTH }} />
              </colgroup>
              <thead>
                <tr>
                  {fit.shown.map((f) => (
                    <th key={f.name} scope="col" className={numberCol(f) ? "num" : undefined}>
                      {fieldTitle(f)}
                    </th>
                  ))}
                  <th scope="col">
                    <span className="sr-only">{t("Line actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {keys.map((key, i) => {
                  const no = i + 1;
                  const editable = rowEditable(key);
                  const bad = s.invalid.get(key);
                  return (
                    <tr
                      key={key}
                      data-line={key}
                      className={
                        [
                          freshKey(key) && "is-new",
                          s.saving.has(key) && "is-saving",
                          (bad?.size || s.errors.has(key)) && "is-bad",
                        ]
                          .filter(Boolean)
                          .join(" ") || undefined
                      }
                      aria-busy={s.saving.has(key) || undefined}
                      onKeyDown={(e) => onKey(key, e)}
                      onBlur={(e) => {
                        const next = e.relatedTarget as Node | null;
                        if (next && e.currentTarget.contains(next)) return;
                        if (sheet === key) return;
                        // A new line nobody typed in (Enter on the last row opened it) goes away.
                        if (freshKey(key) && !s.touched.has(key)) revert(key);
                        else void commit(key, false);
                      }}
                    >
                      {fit.shown.map((f) => {
                        const id = `ln-${model.name}-${key}-${f.name}`;
                        const canType = editable && editableField(model, f, via);
                        return (
                          <td
                            key={f.name}
                            className={
                              [numberCol(f) && "num", canType && "edit"]
                                .filter(Boolean)
                                .join(" ") || undefined
                            }
                          >
                            {canType ? (
                              <>
                                <label className="sr-only" htmlFor={id}>
                                  {t("{field}, line {n}", { field: fieldTitle(f), n: no })}
                                </label>
                                <FieldInput
                                  field={f}
                                  id={id}
                                  value={val(key, f)}
                                  compact
                                  currency={currency}
                                  invalid={bad?.has(f.name)}
                                  onChange={(v) => setValue(key, f, v)}
                                />
                              </>
                            ) : (
                              <Cell
                                field={f}
                                value={rows[i]?.[f.name]}
                                currency={currency}
                                row={byId.get(key)}
                              />
                            )}
                          </td>
                        );
                      })}
                      <td className="lines-act">
                        <div className="lines-act-in">
                          {(sheetOnly.length > 0 || s.errors.has(key)) && (
                            <IconButton
                              icon="more"
                              size="sm"
                              label={t("Edit line {n}", { n: no })}
                              onClick={() => setSheet(key)}
                            />
                          )}
                          {rowDeletable(key) && (
                            <IconButton
                              icon="trash"
                              size="sm"
                              className="lines-del"
                              label={t("Delete line {n}", { n: no })}
                              disabled={s.saving.has(key)}
                              onClick={() => void remove(key)}
                            />
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
              {sums.length > 0 && (
                <tfoot>
                  <tr>
                    {fit.shown.map((f, i) => {
                      const sum = sums.find((x) => x.f === f);
                      return (
                        <td key={f.name} className={sum ? "num" : undefined}>
                          {sum ? (
                            <span className="amount">{sum.text}</span>
                          ) : i === 0 ? (
                            <span className="lines-total">{t("Total")}</span>
                          ) : null}
                        </td>
                      );
                    })}
                    <td />
                  </tr>
                </tfoot>
              )}
            </table>
          )}
      {empty && (
        <p className="lines-empty">
          {creatingParent && canAdd
            ? t("Add the first line; the record is saved with it.")
            : t("No lines yet")}
        </p>
      )}
      {!cards &&
        [...s.errors.entries()]
          .filter(([k]) => keys.includes(k))
          .map(([k, m]) => (
            <p key={k} role="alert" className="error lines-error">
              {t("Line {n}: {reason}", { n: lineNo(k), reason: serverMessage(m) })}
            </p>
          ))}
      {foot(
        cards && sums.length > 0 && keys.length > 0 && (
          <span className="lines-sums">
            {sums.map((x) => (
              <span key={x.f.name}>
                <span className="dim">{sums.length === 1 ? t("Total") : fieldTitle(x.f)}</span>{" "}
                <span className="amount">{x.text}</span>
              </span>
            ))}
          </span>
        ),
      )}
      {sheetEl}
    </section>
  );
}

/** The rows of one parent, from `<child>.search`. */
function useChildRows(
  model: string,
  via: string,
  parentId: string,
  limit: number,
  order: ServerOrder[],
) {
  return useServerRows({
    enabled: !!parentId,
    model,
    filter: parentId ? [{ field: via, op: "eq", value: parentId }] : [],
    order,
    offset: 0,
    limit,
  });
}

/** One line as a card (phones, narrow panels): what it is, how many at what price, the amount. */
function LineCard(p: {
  cols: FieldMeta[];
  values: Record<string, unknown>;
  fresh: boolean;
  currency: string;
  bad: boolean;
  busy: boolean;
  no: number;
  onOpen: () => void;
}) {
  const d = useData();
  const { cols, values, currency } = p;
  const has = (f: FieldMeta | undefined): f is FieldMeta => !!f && !isEmpty(values[f.name]);
  const text = (f: FieldMeta) =>
    f.kind === "ref" && f.to ? d.refText(f.to, String(values[f.name])) : String(values[f.name]);
  const first = cols[0];
  const describe = cols.find((f) => f !== first && f.kind === "text" && has(f));
  const qty = cols.find((f) => f.kind === "number" && /(^|_)(quantity|qty)$/.test(f.name));
  const price = cols.find((f) => f.kind === "money" && !f.computed && /price/.test(f.name));
  const computed = cols.find((f) => f.computed && (f.kind === "money" || f.kind === "decimal"));
  // A journal line has no computed amount: its debit, or else its credit, is the figure.
  const sides = cols.filter((f) => f.kind === "money" && /^(debit|credit)$/.test(f.name));
  const side = sides.find((f) => numeric(values[f.name]) !== 0);
  const amount = computed ?? side;
  const title = has(first) ? first : describe;
  const sub = describe && describe !== title && text(describe) !== (title ? text(title) : "");
  const shown = new Set([first, describe, qty, price, amount, ...sides]);
  const rest = cols.filter((f) => !shown.has(f) && has(f)).slice(0, 2);
  return (
    <li className={`lines-card${p.bad ? " is-bad" : ""}${p.busy ? " is-saving" : ""}`}>
      <button
        type="button"
        className="lines-card-btn"
        onClick={p.onOpen}
        aria-label={t("Edit line {n}", { n: p.no })}
      >
        <span className="lines-card-main">
          <span className="lines-card-title">
            {title ? (
              <Cell field={title} value={values[title.name]} link={false} currency={currency} />
            ) : (
              <span className="dim">{p.fresh ? t("New line") : t("Line {n}", { n: p.no })}</span>
            )}
          </span>
          {sub && describe && <span className="lines-card-sub">{text(describe)}</span>}
          {(has(qty) || has(price) || rest.length > 0) && (
            <span className="lines-card-meta">
              {has(qty) && has(price) ? (
                <span className="amount">
                  {t("{quantity} × {price}", {
                    quantity: fmt.number(numeric(values[qty.name])),
                    price: fmt.money(numeric(values[price.name]), currency),
                  })}
                </span>
              ) : (
                [qty, price].filter(has).map((f) => (
                  <span key={f.name}>
                    <span className="dim">{fieldTitle(f)}</span>{" "}
                    <Cell field={f} value={values[f.name]} link={false} currency={currency} />
                  </span>
                ))
              )}
              {rest.map((f) => (
                <span key={f.name}>
                  <Cell field={f} value={values[f.name]} link={false} currency={currency} />
                </span>
              ))}
            </span>
          )}
        </span>
        {has(amount) && (
          <span className="lines-card-amount">
            {amount === side && <span className="lines-card-side">{fieldTitle(side)}</span>}
            <Cell field={amount} value={values[amount.name]} currency={currency} />
          </span>
        )}
        <Icon name="chevron-right" size={16} />
      </button>
    </li>
  );
}

/** A compact list of child records with a life of their own; each opens on its own page. */
function CompactList(p: {
  model: ModelMeta;
  via: string;
  rows: Row[];
  cols: FieldMeta[];
  currency: string;
  ctx: PresentCtx;
}) {
  const d = useData();
  const { model, via, rows, ctx } = p;
  if (rows.length === 0) return null;
  const state = stateFieldOf(model);
  const date = model.fields.find((f) => f.kind === "date" || f.kind === "datetime");
  const lang = model.fields.find(
    (f) => f.kind === "select" && (f.name === "lang" || f.name === "language"),
  );
  // A title made of the parent and a date only ("Aug 25") says little: a choice of the record
  // (a service's kind, "Repair") names it instead, and the date moves to the line below.
  const own = titleFieldsOf(model).filter((n) => n !== via);
  const dateTitle =
    !model.ui?.title &&
    own.every((n) => {
      const k = model.fields.find((f) => f.name === n)?.kind;
      return k === "date" || k === "datetime";
    });
  const kind = dateTitle
    ? model.fields.find((f) => f.kind === "select" && f !== state && f !== lang)
    : undefined;
  return (
    <ul className="lines-records">
      {rows.slice(0, LIST_LIMIT).map((r) => {
        // The parent is the page around it: it is not part of the title.
        const name =
          kind && !isEmpty(r[kind.name])
            ? optionLabel(String(r[kind.name]), kind)
            : titleOf(model, { ...r, [via]: undefined } as Row, ctx);
        const extra = p.cols
          .filter(
            (f) =>
              f !== state &&
              f !== date &&
              f !== lang &&
              f !== kind &&
              !isEmpty(r[f.name]) &&
              (f.kind === "select" || f.kind === "money"),
          )
          .slice(0, 2);
        const href = d.linkTo(model.name, { id: r.id });
        const body = (
          <>
            <span className="lines-rec-main">
              <span className="lines-rec-title">
                {lang && !isEmpty(r[lang.name]) && (
                  <span className="lines-rec-lang" title={optionLabel(String(r[lang.name]), lang)}>
                    {String(r[lang.name])}
                  </span>
                )}
                {name}
              </span>
              {(extra.length > 0 || (date && !isEmpty(r[date.name]))) && (
                <span className="lines-rec-meta">
                  {extra.map((f) => (
                    <span key={f.name}>
                      <Cell field={f} value={r[f.name]} link={false} currency={p.currency} />
                    </span>
                  ))}
                  {date && !isEmpty(r[date.name]) && (
                    <span>
                      <Cell field={date} value={r[date.name]} link={false} />
                    </span>
                  )}
                </span>
              )}
            </span>
            {state && !isEmpty(r[state.name]) && (
              <Badge field={state} value={String(r[state.name])} />
            )}
          </>
        );
        return (
          <li key={r.id}>
            {href ? (
              <a className="lines-rec" href={href}>
                {body}
                <Icon name="chevron-right" size={16} />
              </a>
            ) : (
              <span className="lines-rec">{body}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Thumbnails in their order; dragged (or moved with Alt+arrow keys) to reorder. */
function ImageGrid(p: {
  model: ModelMeta;
  rows: Row[];
  canMove: boolean;
  canDelete: (id: string) => boolean;
  busy: (id: string) => boolean;
  onOpen: (id: string) => void;
  onDelete: (id: string) => void;
  canAdd: boolean;
  onAdd: () => void;
  title: (r: Row) => string;
}) {
  const d = useData();
  const { model, rows } = p;
  const img = model.fields.find((f) => f.kind === "image");
  const pos = model.fields.find((f) => f.kind === "number" && POSITION.test(f.name));
  const [drag, setDrag] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [order, setOrder] = useState<string[] | null>(null);
  // A new answer from the server replaces the order kept while the moves were saved.
  useEffect(() => setOrder(null), [rows]);
  const ids = order ?? rows.map((r) => r.id);
  const byId = new Map(rows.map((r) => [r.id, r]));
  const moveable = p.canMove && !!pos;

  async function place(next: string[]) {
    if (!pos) return;
    setOrder(next);
    // Each image whose place changed gets its new number, one `update` each.
    for (const [i, id] of next.entries()) {
      const r = byId.get(id);
      if (r && numeric(r[pos.name]) !== i + 1)
        await d.run(`${model.name}.update`, { id, [pos.name]: i + 1 });
    }
  }
  const moveTo = (id: string, to: number) => {
    const from = ids.indexOf(id);
    if (from < 0 || to < 0 || to >= ids.length || from === to) return;
    const next = [...ids];
    next.splice(from, 1);
    next.splice(to, 0, id);
    void place(next);
  };

  return (
    <ul className="lines-grid">
      {ids.map((id, i) => {
        const r = byId.get(id);
        if (!r) return null;
        const label = p.title(r);
        return (
          <li
            key={id}
            className={`lines-tile${drag === id ? " is-drag" : ""}${over === id ? " is-over" : ""}`}
            draggable={moveable}
            aria-busy={p.busy(id) || undefined}
            onDragStart={(e) => {
              setDrag(id);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragEnd={() => {
              setDrag(null);
              setOver(null);
            }}
            onDragOver={(e) => {
              if (!drag || drag === id) return;
              e.preventDefault();
              setOver(id);
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (drag) moveTo(drag, i);
              setDrag(null);
              setOver(null);
            }}
          >
            <button
              type="button"
              className="lines-tile-btn"
              onClick={() => p.onOpen(id)}
              aria-label={t("Edit image {n}: {name}", { n: i + 1, name: label })}
              aria-keyshortcuts={moveable ? "Alt+ArrowLeft Alt+ArrowRight" : undefined}
              onKeyDown={(e) => {
                if (!moveable || !e.altKey) return;
                if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
                  e.preventDefault();
                  moveTo(id, i - 1);
                } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
                  e.preventDefault();
                  moveTo(id, i + 1);
                }
              }}
            >
              {img && !isEmpty(r[img.name]) ? (
                <BlobThumb id={String(r[img.name])} alt={label} size={112} />
              ) : (
                <span className="lines-tile-ph">
                  <Icon name="upload" size={20} />
                </span>
              )}
              <span className="lines-tile-cap">{label}</span>
            </button>
            {moveable && (
              <span className="lines-tile-grip" aria-hidden="true">
                <Icon name="grip" size={14} />
              </span>
            )}
            {p.canDelete(id) && (
              <IconButton
                icon="close"
                size="sm"
                className="lines-tile-del"
                label={t("Delete image {n}", { n: i + 1 })}
                disabled={p.busy(id)}
                onClick={() => p.onDelete(id)}
              />
            )}
          </li>
        );
      })}
      {p.canAdd && (
        <li className="lines-tile add">
          <button type="button" className="lines-tile-btn" onClick={p.onAdd}>
            <span className="lines-tile-ph">
              <Icon name="plus" size={20} />
            </span>
            <span className="lines-tile-cap">{t("Add an image")}</span>
          </button>
        </li>
      )}
    </ul>
  );
}

/** Every field of one line in a sheet: the columns that did not fit, long text, files, images. */
function LineSheet(p: {
  title: string;
  model: ModelMeta;
  via: string;
  fields: FieldMeta[];
  row: Row | null;
  value: (f: FieldMeta) => Edit;
  invalid?: Set<string>;
  error?: string;
  editable: boolean;
  deletable: boolean;
  busy: boolean;
  currency: string;
  onChange: (f: FieldMeta, v: Edit) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const base = useId();
  const { model, via } = p;
  const form = useRef<HTMLFormElement>(null);
  // A new line starts in its first field (the product), after the dialog placed its focus.
  const fresh = p.row === null;
  useEffect(() => {
    if (!fresh) return;
    const id = requestAnimationFrame(() =>
      form.current
        ?.querySelector<HTMLElement>("input:not([readonly]):not([type=hidden]), select, textarea")
        ?.focus(),
    );
    return () => cancelAnimationFrame(id);
  }, [fresh]);
  return (
    <Dialog
      title={p.title}
      onClose={p.onClose}
      variant="sheet"
      className="lines-sheet"
      footer={
        <>
          {p.editable && (
            <Button variant="primary" loading={p.busy} onClick={p.onSave}>
              {p.row ? t("Save") : t("Add")}
            </Button>
          )}
          <Button onClick={p.onClose}>{p.editable ? t("Cancel") : t("Close")}</Button>
          <span className="grow" />
          {p.deletable && (
            <Button variant="ghost" className="danger-text" icon="trash" onClick={p.onDelete}>
              {t("Delete")}
            </Button>
          )}
        </>
      }
    >
      <form
        ref={form}
        className="lines-sheet-form"
        noValidate
        onSubmit={(e) => {
          // The sheet is drawn in a portal: its submit must not reach the parent's form.
          e.preventDefault();
          e.stopPropagation();
          if (p.editable) p.onSave();
        }}
      >
        {p.fields.map((f) => {
          const id = `${base}-${f.name}`;
          const canType = p.editable && editableField(model, f, via);
          return (
            <div key={f.name} className={`field${f.kind === "bool" ? " inline" : ""}`}>
              <label className="flabel" htmlFor={id}>
                {fieldTitle(f)}
                {f.required && canType && (
                  <span className="req" title={t("Required")} aria-hidden="true">
                    *
                  </span>
                )}
              </label>
              {canType ? (
                <FieldInput
                  field={f}
                  id={id}
                  value={p.value(f)}
                  currency={p.currency}
                  invalid={p.invalid?.has(f.name)}
                  onChange={(v) => p.onChange(f, v)}
                />
              ) : (
                <span className="lines-sheet-value" id={id}>
                  <Cell
                    field={f}
                    value={p.row?.[f.name]}
                    currency={p.currency}
                    row={p.row ?? undefined}
                  />
                </span>
              )}
            </div>
          );
        })}
        {p.error && (
          <p role="alert" className="error banner">
            {serverMessage(p.error)}
          </p>
        )}
        {/* Enter in a field saves the line. */}
        <button type="submit" hidden tabIndex={-1} />
      </form>
    </Dialog>
  );
}
