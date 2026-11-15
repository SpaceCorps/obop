import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { Badge as UiBadge, describeFor, fmt, getCurrency, inSentence, t } from "@obop/ui";
import { call, getTime, type FieldMeta, type ModelMeta, type Row } from "../api.ts";
import { BlobThumb } from "../BlobFields.tsx";
import { BulkDialog } from "../BulkDialog.tsx";
import { BULK_CAP, EXPORT_CAP, outcomeOf } from "../bulk.ts";
import { Cell } from "../fields.tsx";
import { useData } from "../data.tsx";
import { readPref, writePref } from "../favorites.ts";
import {
  csvCell,
  download,
  formatDate,
  instantText,
  isEmpty,
  optionLabel,
  plainText,
} from "../format.ts";
import { dayInZone } from "../instants.ts";
import { fieldTitle, fieldTitleByName } from "../labels.ts";
import { PHONE_QUERY, useMedia } from "../media.ts";
import {
  currencyOf,
  listColumns,
  stateFieldOf,
  titleOf,
  toneClassOf,
  type PresentCtx,
} from "../presentation.ts";
import { buildGroups, formatSumRows, sumByCurrency, type Group, type Sort } from "../query.ts";
import { DeleteDialog } from "../DeleteDialog.tsx";
import { nounFor } from "../remove.ts";
import { serverMessage } from "../serverMessage.ts";
import { Pager, Popover, Icon } from "../ui.tsx";
import { canMove, isGoverned } from "../workflow.ts";
import { InlineEdit } from "./InlineEdit.tsx";
import {
  cardMeta,
  cardRoles,
  chooseColumns,
  dueLevel,
  editableField,
  firstColumnMode,
  isMaskedValue,
  listOrder,
  listSummable,
  nextSort,
  fitColumns,
  measureColumn,
  type CardRoles,
} from "./listLayout.ts";
import type { ViewProps } from "./types.ts";

type Item = { t: "group"; g: Group } | { t: "row"; row: Row; ri: number };

function flatten(groups: Group[], collapsed: Set<string>, out: Item[], counter: { n: number }) {
  for (const g of groups) {
    out.push({ t: "group", g });
    if (collapsed.has(g.path)) continue;
    if (g.children.length) flatten(g.children, collapsed, out, counter);
    else for (const row of g.rows) out.push({ t: "row", row, ri: counter.n++ });
  }
}

const isNum = (f: FieldMeta) => f.kind === "number" || f.kind === "money" || f.kind === "decimal";

/** The selection column, in pixels (list.css .list .selcol). */
const SELECT_COL_PX = 52;

/** How long a click on an editable cell waits for a second click, which edits it in place. */
const DOUBLE_MS = 250;

/** A plain click on a link inside a row opens the row; Ctrl, Cmd or Shift leave it to the browser. */
function rowLinkClick(e: MouseEvent) {
  if (e.ctrlKey || e.metaKey || e.shiftKey) e.stopPropagation();
  else e.preventDefault();
}

/** What a tap or click on a record means: open it, or tick it while a phone is selecting. */
type Open = (row: Row) => void;

export function ListView(p: ViewProps) {
  const d = useData();
  const { fields, rows, state, setState } = p;
  const model = p.model;
  // A phone shows one card per record (list.css); a device without hover never waits for a
  // second tap, so a tap opens the record at once.
  const phone = useMedia(PHONE_QUERY);
  const touch = useMedia("(hover: none)");

  // The width the table has decides how many columns it shows (presentation.ts listColumns).
  const root = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(1200);
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const measure = () => {
      // The box of the table without its scroll bar. jsdom measures nothing; 16 px steps (down, so
      // the columns never overflow) keep a resize from redrawing the table at every pixel.
      const w = Math.floor(el.clientWidth / 16) * 16;
      if (w > 0) setWidth((old) => (old === w ? old : w));
    };
    measure();
    if (typeof ResizeObserver !== "function") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Columns the person picked themselves; null keeps the ranking of presentation.ts.
  const prefKey = `obop-list-cols:${model.name}`;
  const [custom, setCustom] = useState<string[] | null>(() =>
    readPref<string[] | null>(prefKey, null),
  );
  // Every column has a width by its kind and head (listLayout.ts columnWidth) and the table is laid
  // out fixed, so the columns that fit are known before anything is drawn: low-ranked ones go
  // until the rest fit, never the title, the first amount or the status. Columns the person picked
  // all stay; the table then scrolls sideways inside its box.
  const stateName = stateFieldOf(model)?.name;
  const { cols, widths } = useMemo(() => {
    const ranked = chooseColumns(model, fields, 100_000, rows, custom);
    return fitColumns(
      ranked,
      custom ? Infinity : width,
      fieldTitle,
      stateName,
      measureColumn(model, ranked),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model, fields, width, rows, custom, stateName, d.lang]);
  const mode = firstColumnMode(model, cols);
  const roles = useMemo(() => cardRoles(model, fields, rows), [model, fields, rows]);
  const order = useMemo(() => listOrder(model, fields), [model, fields]);
  // Withheld from this person (the page leaves them out of `fields`): shown greyed among the columns.
  const withheld = useMemo(
    () =>
      model.fields.filter((f) => f.class !== "secret" && !fields.some((x) => x.name === f.name)),
    [model, fields],
  );

  const ctx = useMemo<PresentCtx>(
    () => ({
      refText: d.refText,
      rowOf: (m, id) => d.store.peek(m)?.find((r) => r.id === id),
      modelOf: (m) => d.models.get(m)?.model,
      moduleId: d.models.get(model.name)?.module.id,
      instanceCurrency: getCurrency(),
    }),
    [d, model.name],
  );
  const currencyFor = (row: Row) => currencyOf(row, model, ctx);
  const today = dayInZone(new Date().toISOString(), getTime().zone);

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<{ id: string; field: string } | null>(null);
  // "Edit in list": a click on a cell edits it in place instead of opening the record.
  const editKey = `obop-list-edit:${model.name}`;
  const [editMode, setEditModeState] = useState(() => readPref<boolean>(editKey, false));
  const setEditMode = (on: boolean) => {
    setEditModeState(on);
    writePref(editKey, on);
    if (!on) setEditing(null);
  };
  // A phone selects records only after "Select", as a phone's own lists do.
  const [selecting, setSelecting] = useState(false);
  const [pos, setPos] = useState({ r: 0, c: 1 });
  const [deleting, setDeleting] = useState(false);
  // "Select all N matching" (D1): the selection is the whole search, not only this page.
  const [allMatching, setAllMatching] = useState(false);
  const [bulk, setBulk] = useState<
    { kind: "delete" } | { kind: "set"; field: FieldMeta; value: string | boolean } | null
  >(null);
  const [exporting, setExporting] = useState<{ done: number; total: number } | null>(null);
  const cancelExport = useRef(false);
  const grid = useRef<HTMLTableSectionElement>(null);
  const pending = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(pending.current), []);

  const live = useMemo(() => {
    const ids = new Set(rows.map((r) => r.id));
    return new Set([...selected].filter((id) => ids.has(id)));
  }, [selected, rows]);
  useEffect(() => {
    if (live.size !== selected.size) setSelected(live);
  }, [live, selected]);

  const levels = state.group
    .map((g) => fields.find((f) => f.name === g))
    .filter((f): f is FieldMeta => !!f);
  const groups = useMemo(
    () => (levels.length ? buildGroups(rows, levels, d.lookup) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, state.group, d.lookup, d.lang],
  );
  const items = useMemo(() => {
    if (!groups.length) return rows.map((row, ri): Item => ({ t: "row", row, ri }));
    const out: Item[] = [];
    flatten(groups, collapsed, out, { n: 0 });
    return out;
  }, [groups, rows, collapsed]);

  // A different search is a different set of records: the page selection and "all matching" end.
  const searchKey = JSON.stringify(state.facets);
  useEffect(() => {
    setAllMatching(false);
  }, [searchKey, model.name]);
  const pageOn = rows.length > 0 && live.size === rows.length;
  const allOn = rows.length > 0 && (allMatching || pageOn);
  const selCount = allMatching ? p.total : live.size;
  const hasSelection = live.size > 0 || allMatching;
  const offerAll = pageOn && !allMatching && p.total > rows.length;
  // Phone: the checkboxes show while selecting; elsewhere always.
  const showSelect = !phone || selecting || hasSelection;
  const sums = cols.map((f) => listSummable(f, model));
  const firstSum = sums.indexOf(true);
  const phoneSum = roles.money && listSummable(roles.money, model) ? roles.money : undefined;
  const nCols = (phone ? 1 : cols.length) + 1;

  // The list's order: the person's sort, else the default order (marked, not chosen).
  const shownSort: (Sort & { chosen: boolean }) | undefined = state.sort
    ? { ...state.sort, chosen: true }
    : order[0] && order[0].field !== "id"
      ? { f: order[0].field, dir: order[0].dir, chosen: false }
      : undefined;
  const sortBy = (f: FieldMeta) =>
    setState({ ...state, sort: nextSort(state.sort, f.name, order) });

  const setColumns = (names: string[]) => {
    setCustom(names);
    writePref(prefKey, names);
    setEditing(null);
  };
  const resetColumns = () => {
    setCustom(null);
    writePref(prefKey, null);
  };
  const toggleColumn = (f: FieldMeta, on: boolean) => {
    const names = cols.map((c) => c.name);
    if (!on) return setColumns(names.filter((n) => n !== f.name));
    // A new column goes before the status, which stays last.
    const last = cols.at(-1);
    const keepLast = last && last.kind === "select" && last.name === roles.state?.name;
    setColumns(keepLast ? [...names.slice(0, -1), f.name, last.name] : [...names, f.name]);
  };

  const toggle = (id: string) => {
    if (allMatching) {
      // Unticking one record leaves "all matching": what is on this page stays selected.
      setAllMatching(false);
      setSelected(new Set(rows.map((r) => r.id).filter((x) => x !== id)));
      return;
    }
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };
  const clearSelection = () => {
    setSelected(new Set());
    setAllMatching(false);
  };

  function cellText(f: FieldMeta, r: Row) {
    return plainText(f, r[f.name], d.refText, f.kind === "money" ? currencyFor(r) : undefined);
  }
  /** A value as a phone card's second line reads it: dates in the region's words, refs by name. */
  function shortText(f: FieldMeta, r: Row): string {
    const v = r[f.name];
    if (isEmpty(v)) return "";
    if (f.kind === "ref" && f.to) return d.refText(f.to, String(v));
    if (f.kind === "date") return formatDate(v);
    if (f.kind === "datetime") return instantText(v);
    if (f.kind === "money") return fmt.money(v, currencyFor(r));
    if (f.kind === "select") return optionLabel(String(v), f);
    return cellText(f, r);
  }
  async function exportCsv() {
    // Selected rows of this page, else everything the search matches, fetched page by page.
    let pick = allMatching ? [] : rows.filter((r) => live.has(r.id));
    if (allMatching || !pick.length) {
      cancelExport.current = false;
      setExporting({ done: 0, total: p.total });
      const got = await p.fetchAll({
        cap: EXPORT_CAP,
        cancelled: () => cancelExport.current,
        onProgress: (done, total) => setExporting({ done, total }),
      });
      setExporting(null);
      if ("error" in got) return d.notify(serverMessage(got.error), "error");
      if (got.cancelled) return d.notify(t("Export cancelled"), "info");
      pick = got.rows;
      if (got.truncated)
        d.notify(
          t("Exported the first {count} of {total} records only", {
            count: fmt.integer(pick.length),
            total: fmt.integer(got.total),
          }),
          "info",
        );
    }
    // A phone shows cards, not columns: its export has every column the desktop would show.
    const out = phone ? listColumns(model, 100_000, { fields }) : cols;
    const head = out.map((f) => csvCell(fieldTitle(f))).join(",");
    const body = pick.map((r) => out.map((f) => csvCell(cellText(f, r))).join(","));
    download(`${model.name}.csv`, [head, ...body].join("\r\n") + "\r\n");
    d.notify(
      t("{count, plural, one {Exported # record} other {Exported # records}}", {
        count: pick.length,
      }),
    );
  }
  async function bulkSet(f: FieldMeta, value: string | boolean) {
    if (allMatching) return setBulk({ kind: "set", field: f, value });
    const ids = [...live];
    const results = await Promise.all(ids.map((id) => p.update(id, { [f.name]: value })));
    const failed = results.filter(Boolean);
    d.notify(
      failed.length
        ? t("{ok} updated, {failed} failed: {reason}", {
            ok: ids.length - failed.length,
            failed: failed.length,
            reason: failed[0] ?? "",
          })
        : t("{count, plural, one {Set {field} on # record} other {Set {field} on # records}}", {
            count: ids.length,
            field: fieldTitle(f),
          }),
      failed.length ? "error" : "ok",
    );
  }

  const canEditCell = (row: Row, f: FieldMeta) =>
    !phone && p.canUpdate && editableField(f) && !isMaskedValue(row[f.name]);
  // The editable cells of a row in column order, for Tab and Shift+Tab while editing.
  const editableCols = (row: Row) => cols.filter((f, i) => i > 0 && canEditCell(row, f));

  const focusCell = (r: number, c: number) => {
    const el = grid.current?.querySelector<HTMLElement>(`td[data-r="${r}"][data-c="${c}"]`);
    el?.focus();
  };
  const onGridKey = (e: KeyboardEvent<HTMLTableSectionElement>) => {
    const td = (e.target as HTMLElement).closest<HTMLElement>("td[data-r]");
    if (!td || e.target !== td || editing) return;
    const r = Number(td.dataset.r);
    const c = Number(td.dataset.c);
    const last = rows.length - 1;
    const move = (nr: number, nc: number) => {
      e.preventDefault();
      focusCell(Math.max(0, Math.min(last, nr)), Math.max(0, Math.min(nCols - 1, nc)));
    };
    const row = items.find((i): i is Extract<Item, { t: "row" }> => i.t === "row" && i.ri === r);
    if (e.key === "ArrowDown") move(r + 1, c);
    else if (e.key === "ArrowUp") move(r - 1, c);
    else if (e.key === "ArrowRight") move(r, c + 1);
    else if (e.key === "ArrowLeft") move(r, c - 1);
    else if (e.key === "Home") move(e.ctrlKey ? 0 : r, 0);
    else if (e.key === "End") move(e.ctrlKey ? last : r, nCols - 1);
    else if (e.key === "Enter" && row) {
      e.preventDefault();
      p.onOpen(row.row.id);
    } else if (e.key === " " && row) {
      e.preventDefault();
      toggle(row.row.id);
    } else if (e.key === "F2" && row && !phone && c >= 1 && c <= cols.length) {
      e.preventDefault();
      const f = cols[c - 1]!;
      if (canEditCell(row.row, f)) setEditing({ id: row.row.id, field: f.name });
    }
  };

  const recordHref = (row: Row) => d.linkTo(model.name, { id: row.id });
  const openInTab = (row: Row) => {
    const href = recordHref(row);
    if (href) window.open(href, "_blank", "noopener");
  };
  // Where the mouse went down: a click that ends elsewhere with text selected was a drag to
  // select that text (to copy it), not a click on the record.
  const down = useRef<{ x: number; y: number } | null>(null);
  const draggedToSelect = (e: MouseEvent) => {
    const d0 = down.current;
    if (!d0 || Math.hypot(e.clientX - d0.x, e.clientY - d0.y) < 5) return false;
    const s = window.getSelection();
    return !!s && !s.isCollapsed;
  };
  const open: Open = (row) => (phone && selecting ? toggle(row.id) : p.onOpen(row.id));

  /**
   * A click anywhere in a row opens the record. "Edit in list" turns a click on an editable cell
   * into editing it; without it a double-click (or F2) does, so a click on such a cell waits a moment
   * for the second one. The first column always opens the record, at once.
   */
  function clickCell(e: MouseEvent<HTMLElement>, row: Row, f: FieldMeta, ci: number) {
    window.clearTimeout(pending.current);
    // The cell, not the words in it, holds the focus: the arrow keys go on from there, and closing
    // the record comes back to it.
    e.currentTarget.focus({ preventScroll: true });
    const editable = ci > 0 && canEditCell(row, f);
    const edit = () => {
      // A double-click also selected the word under the mouse; the input takes over from it.
      window.getSelection()?.removeAllRanges();
      setEditing({ id: row.id, field: f.name });
    };
    if (editable && e.detail >= 2 && !editMode) return edit();
    if (draggedToSelect(e)) return;
    if (e.ctrlKey || e.metaKey || e.shiftKey) return openInTab(row);
    if (editable && editMode) return edit();
    const tap = (e.nativeEvent as PointerEvent).pointerType === "touch" || touch;
    if (!editable || tap) return open(row);
    pending.current = window.setTimeout(() => open(row), DOUBLE_MS);
  }
  const auxCell = (e: MouseEvent, row: Row) => {
    if (e.button === 1) openInTab(row);
  };

  // A governed field moves through its actions, so it is not set in bulk.
  const settableFields = fields.filter(
    (f) => (f.kind === "select" || f.kind === "bool") && !isGoverned(model, f),
  );
  const selectedRecords = rows
    .filter((r) => live.has(r.id))
    .map((r) => ({ id: r.id, label: titleOf(model, r, ctx) }));

  // The dialog lives in the model page: a bulk delete can empty the list, which replaces this view
  // while the dialog still has its results to show.
  const bulkDialog = bulk ? (
    <BulkDialog
      title={
        bulk.kind === "delete"
          ? t("Delete the matching records")
          : t("Set {field} on the matching records", { field: fieldTitle(bulk.field) })
      }
      total={p.total}
      question={(n) =>
        bulk.kind === "delete"
          ? t(
              "{count, plural, one {Delete # record that matches this search? This cannot be undone.} other {Delete # records that match this search? This cannot be undone.}}",
              { count: n },
            )
          : t(
              "{count, plural, one {Set {field} to {value} on # record that matches this search?} other {Set {field} to {value} on # records that match this search?}}",
              {
                count: n,
                field: fieldTitle(bulk.field),
                value:
                  bulk.field.kind === "bool"
                    ? bulk.value
                      ? t("Yes")
                      : t("No")
                    : optionLabel(String(bulk.value), bulk.field),
              },
            )
      }
      confirmLabel={(n) =>
        bulk.kind === "delete"
          ? t("{count, plural, one {Delete # record} other {Delete # records}}", { count: n })
          : t("{count, plural, one {Change # record} other {Change # records}}", { count: n })
      }
      collect={(o) => p.fetchAll(o)}
      labelOf={(row) => titleOf(model, row, ctx)}
      noun={(n) => nounFor(inSentence(p.title), n)}
      apply={async (row) => {
        const label = titleOf(model, row, ctx);
        const r =
          bulk.kind === "delete"
            ? await call(d.token, `${model.name}.delete`, { id: row.id })
            : await call(d.token, `${model.name}.update`, {
                id: row.id,
                [bulk.field.name]: bulk.value,
              });
        return outcomeOf({ id: row.id, label }, r);
      }}
      onClose={() => {
        setBulk(null);
        p.setDialog(null);
      }}
      onDone={() => {
        clearSelection();
        void d.changed(model.name);
      }}
    />
  ) : null;
  const showDialog = p.setDialog;
  useEffect(() => {
    showDialog(bulkDialog);
    // The dialog's inputs are fixed when it opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bulk]);

  /** The value of a cell, as the list draws it. */
  function cellValue(row: Row, f: FieldMeta): ReactNode {
    const v = row[f.name];
    if (isEmpty(v) || isMaskedValue(v)) return <Cell field={f} value={v} />;
    if (f.kind === "ref" && f.to) return <RefText field={f} id={String(v)} />;
    if (f.kind === "select")
      return (
        <UiBadge tone={toneClassOf(f, String(v), model) as `t-${string}`}>
          {optionLabel(String(v), f)}
        </UiBadge>
      );
    const cell = (
      <Cell
        field={f}
        value={v}
        currency={f.kind === "money" ? currencyFor(row) : undefined}
        row={row}
        link={false}
      />
    );
    return <Due level={dueLevel(model, f, row, today)}>{cell}</Due>;
  }

  /**
   * The first column, and the title of a phone card: the record's title (titleOf), or the column's
   * own value where the rest of the title has columns of its own. Never a link to another record.
   */
  function firstCell(row: Row, f: FieldMeta | undefined, asTitle = mode === "title"): ReactNode {
    const v = f ? row[f.name] : undefined;
    const text: ReactNode =
      asTitle || !f ? (
        titleOf(model, row, ctx)
      ) : isEmpty(v) ? (
        ""
      ) : f.kind === "ref" && f.to ? (
        d.refText(f.to, String(v))
      ) : (
        <Cell
          field={f}
          value={v}
          row={row}
          link={false}
          currency={f.kind === "money" ? currencyFor(row) : undefined}
        />
      );
    const img = roles.image && !isEmpty(row[roles.image.name]) ? row[roles.image.name] : null;
    const href = recordHref(row);
    // A record without its own value in the column yet (a draft without its number) is named by
    // the rest of its title, greyed: "Draft · Payroll · 31 Dec".
    const unset = asTitle && !!f && isEmpty(v);
    const body = (
      <>
        {img !== null && <BlobThumb id={String(img)} alt="" size={28} className="row-avatar" />}
        <span className={`rowtitle${unset ? " dim" : ""}`}>
          {text === "" ? <span className="dim">—</span> : text}
        </span>
      </>
    );
    return href ? (
      <a
        className="rowlink"
        href={href}
        tabIndex={-1}
        onClick={rowLinkClick}
        onAuxClick={(e) => e.stopPropagation()}
      >
        {body}
      </a>
    ) : (
      <span className="rowlink">{body}</span>
    );
  }

  const selectAll = (
    <input
      type="checkbox"
      aria-label={t("Select all records")}
      checked={allOn}
      ref={(el) => {
        if (el) el.indeterminate = hasSelection && !allOn;
      }}
      onChange={() => {
        setAllMatching(false);
        setSelected(allOn ? new Set() : new Set(rows.map((r) => r.id)));
      }}
    />
  );

  const bulkbar = (
    <div className="bulkbar" role="toolbar" aria-label={t("Bulk actions")}>
      <strong>{t("{count} selected", { count: fmt.integer(selCount) })}</strong>
      {offerAll && (
        <button className="linkbtn" onClick={() => setAllMatching(true)}>
          {t("Select all {total} matching", { total: fmt.integer(p.total) })}
        </button>
      )}
      {allMatching && (
        <span className="hint" role="status">
          {p.total > BULK_CAP
            ? t("All {total} matching records are selected; bulk actions use the first {cap}.", {
                total: fmt.integer(p.total),
                cap: fmt.integer(BULK_CAP),
              })
            : t("All {total} matching records are selected.", {
                total: fmt.integer(p.total),
              })}
        </span>
      )}
      {p.canUpdate && settableFields.length > 0 && (
        <Popover label={t("Set…")} ariaLabel={t("Set a field on the selected records")}>
          {(close) => (
            <div className="menu-list">
              {settableFields.map((f) => (
                <div key={f.name} role="group" aria-label={fieldTitle(f)}>
                  <h4>{fieldTitle(f)}</h4>
                  {(f.kind === "bool" ? ["true", "false"] : (f.options ?? [])).map((o) => (
                    <button
                      key={o}
                      onClick={() => {
                        close();
                        void bulkSet(f, f.kind === "bool" ? o === "true" : o);
                      }}
                    >
                      {f.kind === "bool" ? (o === "true" ? t("Yes") : t("No")) : optionLabel(o, f)}
                    </button>
                  ))}
                </div>
              ))}
            </div>
          )}
        </Popover>
      )}
      {/* A phone has room for icons only; "Done" there ends the selection. */}
      <button
        className={`btn small${phone ? " icon-only" : ""}`}
        disabled={!!exporting}
        onClick={() => void exportCsv()}
        aria-label={phone ? t("Export selected") : undefined}
        title={phone ? t("Export selected") : undefined}
      >
        <Icon name="download" size={13} />
        {!phone && <> {t("Export selected")}</>}
      </button>
      {p.canDelete && (
        <button
          className={`btn small danger-text${phone ? " icon-only" : ""}`}
          onClick={() => (allMatching ? setBulk({ kind: "delete" }) : setDeleting(true))}
          aria-label={phone ? t("Delete") : undefined}
          title={phone ? t("Delete") : undefined}
        >
          <Icon name="trash" size={13} />
          {!phone && <> {t("Delete")}</>}
        </button>
      )}
      {!phone && (
        <button className="linkbtn" onClick={clearSelection}>
          {t("Clear")}
        </button>
      )}
    </div>
  );

  const exportStatus = exporting && (
    <span className="hint" role="status" aria-live="polite">
      {t("Exporting {done} of {total}…", {
        done: fmt.integer(exporting.done),
        total: fmt.integer(exporting.total),
      })}{" "}
      <button className="linkbtn" onClick={() => (cancelExport.current = true)}>
        {t("Cancel")}
      </button>
    </span>
  );

  const exportButton = (
    <button
      className={`btn small${phone ? " icon-only" : ""}`}
      onClick={() => void exportCsv()}
      aria-label={t("Export all as CSV")}
      title={phone ? t("Export all as CSV") : undefined}
      disabled={!!exporting}
    >
      <Icon name="download" size={13} />
      {!phone && <> {t("Export CSV")}</>}
    </button>
  );

  const columnsMenu = (
    <Popover
      label={
        <>
          <Icon name="columns" size={14} /> {t("Columns")}
        </>
      }
      ariaLabel={t("Choose columns")}
      align="end"
      className="btn small"
    >
      {() => (
        <div className="menu-list colmenu">
          {fields.map((f) => {
            const on = cols.some((c) => c.name === f.name);
            return (
              <label key={f.name} className="fp-item">
                <input
                  type="checkbox"
                  checked={on}
                  disabled={on && cols.length === 1}
                  onChange={(e) => toggleColumn(f, e.target.checked)}
                />
                <span>{fieldTitle(f)}</span>
              </label>
            );
          })}
          {withheld.map((f) => (
            <label key={f.name} className="fp-item withheld" title={t("Withheld from you")}>
              <input type="checkbox" checked={false} disabled />
              <span>{fieldTitle(f)}</span>
              <Icon name="lock" size={12} />
              <span className="sr-only">{t("Withheld from you")}</span>
            </label>
          ))}
          {custom && (
            <button className="linkbtn colreset" onClick={resetColumns}>
              {t("Reset columns")}
            </button>
          )}
        </div>
      )}
    </Popover>
  );

  const sortable = useMemo(() => listColumns(model, 100_000, { fields }), [model, fields]);
  const sortMenu = (
    <Popover
      label={
        <>
          <Icon name="sort" size={14} />{" "}
          {shownSort ? fieldTitleByName(shownSort.f, fields) : t("Sort")}
          {shownSort && (
            <span className="sortmark on" aria-hidden="true">
              {shownSort.dir === "desc" ? "▼" : "▲"}
            </span>
          )}
        </>
      }
      ariaLabel={t("Sort by")}
      className="btn small sortbtn"
    >
      {(close) => (
        <div className="menu-list sortmenu" role="group" aria-label={t("Sort by")}>
          <button
            aria-pressed={!state.sort}
            onClick={() => {
              close();
              setState({ ...state, sort: undefined });
            }}
          >
            {t("Default order")}
          </button>
          {sortable.map((f) => {
            const on = state.sort?.f === f.name;
            return (
              <button
                key={f.name}
                aria-pressed={on}
                onClick={() => {
                  close();
                  // Numbers and dates start with the largest, words with A.
                  const first = isNum(f) || f.kind === "date" || f.kind === "datetime";
                  setState({
                    ...state,
                    sort: on
                      ? { f: f.name, dir: state.sort!.dir === "asc" ? "desc" : "asc" }
                      : { f: f.name, dir: first ? "desc" : "asc" },
                  });
                }}
              >
                <span>{fieldTitle(f)}</span>
                {on && (
                  <span className="sortmark on" aria-hidden="true">
                    {state.sort!.dir === "desc" ? "▼" : "▲"}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </Popover>
  );

  const selCell = (row: Row, ri: number) => (
    <td
      role="gridcell"
      className="selcol"
      data-r={ri}
      data-c={0}
      tabIndex={pos.r === ri && pos.c === 0 ? 0 : -1}
      onFocus={() => setPos({ r: ri, c: 0 })}
      // The whole cell selects, not only the 17 px box (the box itself has onChange).
      onClick={(e) => {
        if (e.target === e.currentTarget) toggle(row.id);
      }}
    >
      <input
        type="checkbox"
        tabIndex={-1}
        aria-label={t("Select record")}
        checked={allMatching || live.has(row.id)}
        onChange={() => toggle(row.id)}
      />
    </td>
  );

  return (
    <div className={`listview${phone ? " phone" : ""}`}>
      <div className="listbar">
        {phone && selecting && (
          <label className="phone-all">
            {selectAll}
            <span>{t("All")}</span>
          </label>
        )}
        {hasSelection
          ? bulkbar
          : phone
            ? sortMenu
            : p.canUpdate && (
                <button
                  className={`btn small edittoggle${editMode ? " on" : ""}`}
                  aria-pressed={editMode}
                  onClick={() => setEditMode(!editMode)}
                  title={t("Click a cell to change it in place; the first column opens the record")}
                >
                  <Icon name="edit" size={13} /> {t("Edit in list")}
                </button>
              )}
        {exportStatus}
        <span className="grow" />
        {phone && (
          <button
            className="btn small"
            aria-pressed={selecting}
            onClick={() => {
              if (selecting) clearSelection();
              setSelecting(!selecting);
            }}
          >
            {selecting ? t("Done") : t("Select")}
          </button>
        )}
        {/* On a phone the page's ⋯ menu exports (ControlPanel onExport). */}
        {!hasSelection && !phone && exportButton}
        {!phone && columnsMenu}
      </div>
      <div className="tablewrap" ref={root}>
        <table
          className={`table list${editMode && !phone ? " editmode" : ""}${phone ? "" : " fitted"}`}
          role="grid"
          aria-label={t("{model} list", { model: p.title })}
          aria-rowcount={p.total + 1}
          style={
            phone
              ? undefined
              : { minInlineSize: `${widths.reduce((a, b) => a + b, SELECT_COL_PX)}px` }
          }
        >
          {!phone && (
            <colgroup>
              {showSelect && <col style={{ inlineSize: `${SELECT_COL_PX}px` }} />}
              {cols.map((f, i) => (
                <col key={f.name} style={{ inlineSize: `${widths[i]}px` }} />
              ))}
            </colgroup>
          )}
          <thead className={phone ? "sr-only" : undefined}>
            <tr role="row">
              {showSelect && (
                <th className="selcol" role="columnheader" scope="col">
                  {/* A phone has its own "All" above the cards. */}
                  {!phone && selectAll}
                </th>
              )}
              {phone ? (
                <th role="columnheader" scope="col">
                  {p.title}
                </th>
              ) : (
                cols.map((f, ci) => {
                  const s = shownSort?.f === f.name ? shownSort : undefined;
                  return (
                    <th
                      key={f.name}
                      role="columnheader"
                      scope="col"
                      aria-sort={
                        s?.dir === "asc" ? "ascending" : s?.dir === "desc" ? "descending" : "none"
                      }
                      className={`${isNum(f) ? "num" : ""}${ci === 0 ? " titlecol" : ""}`}
                      title={describeFor(f)}
                    >
                      <button className="thbtn" onClick={() => sortBy(f)}>
                        <span className="thlabel">{fieldTitle(f)}</span>
                        <span
                          className={`sortmark${s ? (s.chosen ? " on" : " dflt") : ""}`}
                          aria-hidden="true"
                        >
                          {s?.dir === "desc" ? "▼" : "▲"}
                        </span>
                      </button>
                    </th>
                  );
                })
              )}
            </tr>
          </thead>
          <tbody
            ref={grid}
            onKeyDown={onGridKey}
            onMouseDown={(e) => (down.current = { x: e.clientX, y: e.clientY })}
          >
            {items.map((it) =>
              it.t === "group" ? (
                <GroupRow
                  key={it.g.path}
                  g={it.g}
                  phone={phone}
                  showSelect={showSelect}
                  cols={cols}
                  sums={sums}
                  firstSum={firstSum}
                  phoneSum={phoneSum}
                  collapsed={collapsed.has(it.g.path)}
                  sumOf={(f, rs) => <Sum field={f} rows={rs} currencyFor={currencyFor} />}
                  onToggle={() =>
                    setCollapsed((c) => {
                      const n = new Set(c);
                      if (n.has(it.g.path)) n.delete(it.g.path);
                      else n.add(it.g.path);
                      return n;
                    })
                  }
                />
              ) : (
                <tr
                  key={it.row.id}
                  role="row"
                  aria-rowindex={it.ri + 2}
                  aria-selected={allMatching || live.has(it.row.id)}
                  aria-current={it.row.id === p.selectedId || undefined}
                  className={`${it.row.id === p.selectedId ? "current" : ""}${phone ? " lc-row" : ""}`}
                >
                  {showSelect && selCell(it.row, it.ri)}
                  {phone ? (
                    <td
                      role="gridcell"
                      className="lcard"
                      data-r={it.ri}
                      data-c={1}
                      tabIndex={pos.r === it.ri && pos.c === 1 ? 0 : -1}
                      onFocus={() => setPos({ r: it.ri, c: 1 })}
                      onClick={(e) => {
                        if (e.ctrlKey || e.metaKey || e.shiftKey) return openInTab(it.row);
                        open(it.row);
                      }}
                    >
                      <PhoneCard
                        model={model}
                        row={it.row}
                        roles={roles}
                        title={firstCell(it.row, roles.title, true)}
                        text={(f) => shortText(f, it.row)}
                        money={(f) =>
                          f.kind === "money"
                            ? fmt.money(it.row[f.name], currencyFor(it.row))
                            : cellText(f, it.row)
                        }
                        today={today}
                      />
                    </td>
                  ) : (
                    cols.map((f, ci) => {
                      const edit = editing?.id === it.row.id && editing.field === f.name;
                      const editable = ci > 0 && canEditCell(it.row, f);
                      return (
                        <td
                          key={f.name}
                          role="gridcell"
                          data-r={it.ri}
                          data-c={ci + 1}
                          data-f={f.name}
                          tabIndex={pos.r === it.ri && pos.c === ci + 1 ? 0 : -1}
                          className={[
                            isNum(f) ? "num" : "",
                            ci === 0 ? "titlecol" : "",
                            f.kind === "ref" ? "refcol" : "",
                            edit ? "editing" : "",
                            editable && editMode ? "editable" : "",
                          ]
                            .filter(Boolean)
                            .join(" ")}
                          onFocus={() => setPos({ r: it.ri, c: ci + 1 })}
                          onClick={(e) => !edit && clickCell(e, it.row, f, ci)}
                          onAuxClick={(e) => auxCell(e, it.row)}
                        >
                          {edit ? (
                            <InlineEdit
                              field={f}
                              row={it.row}
                              update={
                                isGoverned(model, f)
                                  ? (id, patch) => p.moveState(id, f.name, String(patch[f.name]))
                                  : p.update
                              }
                              optionState={
                                isGoverned(model, f)
                                  ? (o) => {
                                      const m = canMove(
                                        model,
                                        f,
                                        it.row[f.name] as string,
                                        o,
                                        p.title,
                                        it.row,
                                      );
                                      return {
                                        disabled: !m.allowed && !m.current,
                                        title: m.reason || undefined,
                                      };
                                    }
                                  : undefined
                              }
                              advance={(dir) => {
                                const list = editableCols(it.row);
                                const next = list[list.indexOf(f) + dir];
                                setEditing(next ? { id: it.row.id, field: next.name } : null);
                                if (!next) focusCell(it.ri, ci + 1);
                              }}
                              done={() => {
                                setEditing(null);
                                focusCell(it.ri, ci + 1);
                              }}
                            />
                          ) : ci === 0 ? (
                            firstCell(it.row, f)
                          ) : (
                            cellValue(it.row, f)
                          )}
                        </td>
                      );
                    })
                  )}
                </tr>
              ),
            )}
          </tbody>
          {(phone ? !!phoneSum : firstSum >= 0) && (
            <tfoot>
              <tr role="row">
                <td
                  colSpan={
                    phone ? (showSelect ? 2 : 1) : (showSelect ? 1 : 0) + Math.max(1, firstSum)
                  }
                  role="gridcell"
                  className="foot-label"
                >
                  <strong>
                    {p.pager && p.total > rows.length ? t("Total on this page") : t("Total")}
                  </strong>{" "}
                  <span className="hint">({fmt.integer(rows.length)})</span>
                  {phone && phoneSum && (
                    <span className="foot-sum amount">
                      <Sum field={phoneSum} rows={rows} currencyFor={currencyFor} />
                    </span>
                  )}
                </td>
                {!phone &&
                  cols.slice(Math.max(1, firstSum)).map((f, i) => {
                    const on = sums[i + Math.max(1, firstSum)];
                    return (
                      <td
                        key={f.name}
                        role="gridcell"
                        className={`${isNum(f) ? "num" : ""} agg`}
                        data-f={f.name}
                      >
                        {on ? <Sum field={f} rows={rows} currencyFor={currencyFor} /> : ""}
                      </td>
                    );
                  })}
              </tr>
            </tfoot>
          )}
        </table>
      </div>
      {p.pager && (
        <Pager
          offset={p.pager.offset}
          limit={p.pager.limit}
          total={p.total}
          busy={p.pager.busy}
          onOffset={p.pager.setOffset}
          onSize={p.pager.setLimit}
        />
      )}
      {deleting && (
        <DeleteDialog
          model={model}
          noun={inSentence(p.title)}
          records={selectedRecords}
          onClose={() => setDeleting(false)}
          onDone={() => setSelected(new Set())}
        />
      )}
    </div>
  );
}

/** A group head of a grouped list: the value, its count, and the sums of the columns that add up. */
function GroupRow(p: {
  g: Group;
  phone: boolean;
  showSelect: boolean;
  cols: FieldMeta[];
  sums: boolean[];
  firstSum: number;
  phoneSum?: FieldMeta;
  collapsed: boolean;
  sumOf: (f: FieldMeta, rows: Row[]) => ReactNode;
  onToggle: () => void;
}) {
  const { g } = p;
  const lead = p.phone ? 1 : p.firstSum < 0 ? p.cols.length : Math.max(1, p.firstSum);
  return (
    <tr role="row" className="ghead">
      <td
        colSpan={lead + (p.showSelect ? 1 : 0)}
        role="gridcell"
        style={{ paddingInlineStart: `${0.75 + g.depth * 1.25}rem` }}
      >
        <button className="gbtn" aria-expanded={!p.collapsed} onClick={p.onToggle}>
          <span className={`caret${p.collapsed ? "" : " down"}`}>
            <Icon name="chevron-right" size={13} />
          </span>
          <span className="gname">
            {g.field.kind === "select" && g.key ? <Cell field={g.field} value={g.key} /> : g.label}
          </span>
          <span className="chip">{g.rows.length}</span>
        </button>
        {p.phone && p.phoneSum && (
          <span className="gsum amount">{p.sumOf(p.phoneSum, g.rows)}</span>
        )}
      </td>
      {!p.phone &&
        p.firstSum >= 0 &&
        p.cols.slice(lead).map((f, i) => (
          <td key={f.name} role="gridcell" className={`${isNum(f) ? "num" : ""} agg`}>
            {p.sums[i + lead] ? p.sumOf(f, g.rows) : ""}
          </td>
        ))}
    </tr>
  );
}

/**
 * The total of a column. Amounts in several currencies are never added together: each currency
 * is a line of its own, largest group first, so a narrow column still shows every one.
 */
function Sum(p: { field: FieldMeta; rows: Row[]; currencyFor: (row: Row) => string | undefined }) {
  if (p.field.kind !== "money") return <>{formatSumRows(p.field, p.rows)}</>;
  const parts = sumByCurrency(p.rows, p.field.name, p.currencyFor);
  if (parts.length === 1) return <>{fmt.money(parts[0]![1], parts[0]![0])}</>;
  return (
    <span className="sums">
      {parts.map(([c, n]) => (
        <span key={c}>{fmt.money(n, c)}</span>
      ))}
    </span>
  );
}

/** A ref in a list cell: its label as text, with an arrow that follows it on hover. */
function RefText({ field, id }: { field: FieldMeta; id: string }) {
  const d = useData();
  const to = field.to ?? "";
  const text = d.refText(to, id);
  const target = d.linkTo(to, { id });
  if (!target) return <span className="reftext">{text}</span>;
  return (
    <a
      className="reftext"
      href={target}
      tabIndex={-1}
      onClick={(e) => {
        // The arrow follows the ref, and so do Ctrl, Cmd and Shift; a plain click opens the row.
        if ((e.target as Element).closest(".refgo") || e.ctrlKey || e.metaKey || e.shiftKey)
          e.stopPropagation();
        else e.preventDefault();
      }}
      onAuxClick={(e) => e.stopPropagation()}
    >
      <span className="reftext-label">{text}</span>
      <span className="refgo" title={t("Open {name}", { name: text })} aria-hidden="true">
        <Icon name="external" size={12} />
      </span>
    </a>
  );
}

/** A date that is a deadline: red when past, amber when close (`ui.warn_before_days`). */
function Due({ level, children }: { level: "past" | "soon" | null; children: ReactNode }) {
  if (!level) return <>{children}</>;
  return (
    <span className={`due ${level}`}>
      {children}
      <span className="sr-only"> ({level === "past" ? t("overdue") : t("due soon")})</span>
    </span>
  );
}

/**
 * A record on a phone: the title in bold with the status on the right; under it the number, the
 * main ref and the date, muted, with the amount on the right.
 */
function PhoneCard(p: {
  model: ModelMeta;
  row: Row;
  roles: CardRoles;
  title: ReactNode;
  text: (f: FieldMeta) => string;
  money: (f: FieldMeta) => string;
  today: string;
}) {
  const { row, roles } = p;
  const state = roles.state && !isEmpty(row[roles.state.name]) ? roles.state : undefined;
  // A title made of the ref and the date (a draft without its number) already says them.
  const bare = !!roles.title && isEmpty(row[roles.title.name]);
  const meta = cardMeta(roles, row).filter(
    (f) => !bare || (f.kind !== "ref" && f.kind !== "date" && f.kind !== "datetime"),
  );
  const named = (f: FieldMeta, text: string) =>
    roles.namedMoney && f.kind === "money" ? `${fieldTitle(f)} ${text}` : text;
  const amount =
    roles.money && !isEmpty(row[roles.money.name]) ? named(roles.money, p.money(roles.money)) : "";
  return (
    <div className="lc">
      <div className="lc-top">
        <span className="lc-title">{p.title}</span>
        {state && (
          <UiBadge tone={toneClassOf(state, String(row[state.name]), p.model) as `t-${string}`}>
            {optionLabel(String(row[state.name]), state)}
          </UiBadge>
        )}
      </div>
      {(meta.length > 0 || amount) && (
        <div className="lc-sub">
          <span className="lc-meta">
            {meta.map((f, i) => (
              <span key={f.name}>
                {i > 0 && <span aria-hidden="true"> · </span>}
                <Due level={dueLevel(p.model, f, row, p.today)}>{named(f, p.text(f))}</Due>
              </span>
            ))}
          </span>
          {amount && <span className="lc-money amount">{amount}</span>}
        </div>
      )}
    </div>
  );
}
