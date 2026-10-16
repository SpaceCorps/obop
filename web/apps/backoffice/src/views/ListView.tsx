import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { describeFor, inSentence, t } from "@obop/ui";
import type { FieldMeta, Row } from "../api.ts";
import { Cell } from "../fields.tsx";
import { useData } from "../data.tsx";
import { readPref, writePref } from "../favorites.ts";
import { csvCell, download, isState, optionLabel, plainText, rowCurrency } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { buildGroups, formatSumRows, type Group } from "../query.ts";
import { DeleteDialog } from "../DeleteDialog.tsx";
import { Pager, Popover, Icon } from "../ui.tsx";
import { canMove, isGoverned } from "../workflow.ts";
import { InlineEdit } from "./InlineEdit.tsx";
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

/** Everything but long text, up to 8 columns, and the status field always makes it. */
function defaultColumns(fields: FieldMeta[]): string[] {
  const short = fields.filter((f) => f.kind !== "longtext");
  const status = short.find(isState);
  const picked = short.slice(0, status ? 7 : 8);
  if (status && !picked.includes(status)) picked.push(status);
  return picked.map((f) => f.name);
}

const isAgg = (f: FieldMeta) => f.kind === "number" || f.kind === "money";

export function ListView(p: ViewProps) {
  const d = useData();
  const { fields, rows, state, setState } = p;
  const prefKey = `obop-cols:${p.model.name}`;
  const [colNames, setColNames] = useState<string[]>(() =>
    readPref<string[]>(prefKey, defaultColumns(fields)),
  );
  const cols = fields.filter((f) => colNames.includes(f.name));
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [editing, setEditing] = useState<{ id: string; field: string } | null>(null);
  const [pos, setPos] = useState({ r: 0, c: 1 });
  const [deleting, setDeleting] = useState(false);
  const grid = useRef<HTMLTableSectionElement>(null);

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
    [rows, state.group, d.lookup],
  );
  const items = useMemo(() => {
    if (!groups.length) return rows.map((row, ri): Item => ({ t: "row", row, ri }));
    const out: Item[] = [];
    flatten(groups, collapsed, out, { n: 0 });
    return out;
  }, [groups, rows, collapsed]);

  const allOn = rows.length > 0 && live.size === rows.length;
  const nCols = cols.length + 2;
  const firstAgg = cols.findIndex(isAgg);
  const span = firstAgg < 0 ? nCols : firstAgg + 1;

  const setColumns = (names: string[]) => {
    setColNames(names);
    writePref(prefKey, names);
  };
  const sortBy = (f: FieldMeta) => {
    const s = state.sort;
    const next =
      !s || s.f !== f.name
        ? { f: f.name, dir: "asc" as const }
        : s.dir === "asc"
          ? { f: f.name, dir: "desc" as const }
          : undefined;
    setState({ ...state, sort: next });
  };
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  function cellText(f: FieldMeta, r: Row) {
    return plainText(f, r[f.name], d.refText, rowCurrency(r));
  }
  async function exportCsv() {
    // Selected rows, else everything the search matches (not only the page on screen).
    let pick = rows.filter((r) => live.has(r.id));
    if (!pick.length) {
      const all = await p.exportAll();
      pick = all.rows;
      if (all.truncated)
        d.notify(t("Exported the first {count} records only", { count: pick.length }), "info");
    }
    const head = cols.map((f) => csvCell(fieldTitle(f))).join(",");
    const body = pick.map((r) => cols.map((f) => csvCell(cellText(f, r))).join(","));
    download(`${p.model.name}.csv`, [head, ...body].join("\r\n") + "\r\n");
    d.notify(
      t("{count, plural, one {Exported # record} other {Exported # records}}", {
        count: pick.length,
      }),
    );
  }
  async function bulkSet(f: FieldMeta, value: string | boolean) {
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
    } else if (e.key === "F2" && row && p.canUpdate && c >= 1 && c <= cols.length) {
      e.preventDefault();
      const f = cols[c - 1]!;
      if (!isMaskedVal(row.row[f.name])) setEditing({ id: row.row.id, field: f.name });
    }
  };

  function clickCell(r: Row, f: FieldMeta, ci: number) {
    // The first column opens the record; the other cells edit in place when allowed.
    if (p.canUpdate && ci > 0 && !isMaskedVal(r[f.name])) setEditing({ id: r.id, field: f.name });
    else p.onOpen(r.id);
  }

  // A governed field moves through its actions, so it is not set in bulk.
  const settableFields = p.fields.filter(
    (f) => (f.kind === "select" || f.kind === "bool") && !isGoverned(p.model, f),
  );
  const selectedRecords = rows
    .filter((r) => live.has(r.id))
    .map((r) => ({ id: r.id, label: d.recordLabel(p.model.name, r) }));

  return (
    <div className="listview">
      <div className="listbar">
        {live.size > 0 ? (
          <div className="bulkbar" role="toolbar" aria-label={t("Bulk actions")}>
            <strong>{t("{count} selected", { count: live.size })}</strong>
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
                            {f.kind === "bool"
                              ? o === "true"
                                ? t("Yes")
                                : t("No")
                              : optionLabel(o, f)}
                          </button>
                        ))}
                      </div>
                    ))}
                  </div>
                )}
              </Popover>
            )}
            <button className="btn small" onClick={() => void exportCsv()}>
              <Icon name="download" size={13} /> {t("Export selected")}
            </button>
            {p.canDelete && (
              <button className="btn small danger-text" onClick={() => setDeleting(true)}>
                <Icon name="trash" size={13} /> {t("Delete")}
              </button>
            )}
            <button className="linkbtn" onClick={() => setSelected(new Set())}>
              {t("Clear")}
            </button>
          </div>
        ) : (
          <span className="hint">
            {p.canUpdate ? t("Click a cell to edit it, the first column opens the record.") : ""}
          </span>
        )}
        <span className="grow" />
        <button
          className="btn small"
          onClick={() => void exportCsv()}
          aria-label={t("Export all as CSV")}
          hidden={live.size > 0}
        >
          <Icon name="download" size={13} /> {t("Export CSV")}
        </button>
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
            <div className="menu-list">
              {fields.map((f) => (
                <label key={f.name} className="fp-item">
                  <input
                    type="checkbox"
                    checked={colNames.includes(f.name)}
                    disabled={colNames.length === 1 && colNames.includes(f.name)}
                    onChange={(e) =>
                      setColumns(
                        e.target.checked
                          ? fields
                              .filter((x) => colNames.includes(x.name) || x.name === f.name)
                              .map((x) => x.name)
                          : colNames.filter((n) => n !== f.name),
                      )
                    }
                  />
                  <span>{fieldTitle(f)}</span>
                </label>
              ))}
            </div>
          )}
        </Popover>
      </div>
      <div className="tablewrap">
        <table
          className="table list"
          role="grid"
          aria-label={t("{model} list", { model: p.title })}
          aria-rowcount={p.total + 1}
        >
          <thead>
            <tr role="row">
              <th className="selcol" role="columnheader" scope="col">
                <input
                  type="checkbox"
                  aria-label={t("Select all records")}
                  checked={allOn}
                  ref={(el) => {
                    if (el) el.indeterminate = live.size > 0 && !allOn;
                  }}
                  onChange={() => setSelected(allOn ? new Set() : new Set(rows.map((r) => r.id)))}
                />
              </th>
              {cols.map((f) => {
                const s = state.sort?.f === f.name ? state.sort.dir : undefined;
                return (
                  <th
                    key={f.name}
                    role="columnheader"
                    scope="col"
                    aria-sort={s === "asc" ? "ascending" : s === "desc" ? "descending" : "none"}
                    className={isAgg(f) ? "num" : ""}
                    title={describeFor(f)}
                  >
                    <button className="thbtn" onClick={() => sortBy(f)}>
                      {fieldTitle(f)}
                      {f.required && (
                        <span className="req" aria-hidden="true">
                          {" "}
                          *
                        </span>
                      )}
                      <span className={`sortmark${s ? " on" : ""}`} aria-hidden="true">
                        {s === "desc" ? "▼" : "▲"}
                      </span>
                    </button>
                  </th>
                );
              })}
              <th className="opencol" role="columnheader" scope="col">
                <span className="sr-only">{t("Open")}</span>
              </th>
            </tr>
          </thead>
          <tbody ref={grid} onKeyDown={onGridKey}>
            {items.map((it) =>
              it.t === "group" ? (
                <tr key={it.g.path} role="row" className="ghead">
                  <td
                    colSpan={span}
                    role="gridcell"
                    style={{ paddingInlineStart: `${0.75 + it.g.depth * 1.25}rem` }}
                  >
                    <button
                      className="gbtn"
                      aria-expanded={!collapsed.has(it.g.path)}
                      onClick={() =>
                        setCollapsed((c) => {
                          const n = new Set(c);
                          if (n.has(it.g.path)) n.delete(it.g.path);
                          else n.add(it.g.path);
                          return n;
                        })
                      }
                    >
                      <span className={`caret${collapsed.has(it.g.path) ? "" : " down"}`}>
                        <Icon name="chevron-right" size={13} />
                      </span>
                      <span className="gname">
                        {it.g.field.kind === "select" && it.g.key ? (
                          <Cell field={it.g.field} value={it.g.key} />
                        ) : (
                          it.g.label
                        )}
                      </span>
                      <span className="chip">{it.g.rows.length}</span>
                    </button>
                  </td>
                  {firstAgg >= 0 &&
                    cols.slice(firstAgg).map((f) => (
                      <td key={f.name} role="gridcell" className="num agg">
                        {isAgg(f) ? formatSumRows(f, it.g.rows) : ""}
                      </td>
                    ))}
                  {firstAgg >= 0 && <td role="gridcell" />}
                </tr>
              ) : (
                <tr
                  key={it.row.id}
                  role="row"
                  aria-rowindex={it.ri + 2}
                  aria-selected={live.has(it.row.id)}
                  aria-current={it.row.id === p.selectedId || undefined}
                  className={it.row.id === p.selectedId ? "current" : ""}
                >
                  <td
                    role="gridcell"
                    className="selcol"
                    data-r={it.ri}
                    data-c={0}
                    tabIndex={pos.r === it.ri && pos.c === 0 ? 0 : -1}
                    onFocus={() => setPos({ r: it.ri, c: 0 })}
                  >
                    <input
                      type="checkbox"
                      tabIndex={-1}
                      aria-label={t("Select record")}
                      checked={live.has(it.row.id)}
                      onChange={() => toggle(it.row.id)}
                    />
                  </td>
                  {cols.map((f, ci) => {
                    const edit = editing?.id === it.row.id && editing.field === f.name;
                    return (
                      <td
                        key={f.name}
                        role="gridcell"
                        data-r={it.ri}
                        data-c={ci + 1}
                        tabIndex={pos.r === it.ri && pos.c === ci + 1 ? 0 : -1}
                        className={`${isAgg(f) ? "num" : ""}${edit ? " editing" : ""}${p.canUpdate && ci > 0 ? " editable" : ""}`}
                        onFocus={() => setPos({ r: it.ri, c: ci + 1 })}
                        onClick={() => !edit && clickCell(it.row, f, ci)}
                      >
                        {edit ? (
                          <InlineEdit
                            field={f}
                            row={it.row}
                            update={
                              isGoverned(p.model, f)
                                ? (id, patch) => p.moveState(id, f.name, String(patch[f.name]))
                                : p.update
                            }
                            optionState={
                              isGoverned(p.model, f)
                                ? (o) => {
                                    const m = canMove(
                                      p.model,
                                      f,
                                      it.row[f.name] as string,
                                      o,
                                      p.title,
                                    );
                                    return {
                                      disabled: !m.allowed && !m.current,
                                      title: m.reason || undefined,
                                    };
                                  }
                                : undefined
                            }
                            done={() => {
                              setEditing(null);
                              focusCell(it.ri, ci + 1);
                            }}
                          />
                        ) : (
                          <Cell field={f} value={it.row[f.name]} currency={rowCurrency(it.row)} />
                        )}
                      </td>
                    );
                  })}
                  <td
                    role="gridcell"
                    className="opencol"
                    data-r={it.ri}
                    data-c={cols.length + 1}
                    tabIndex={pos.r === it.ri && pos.c === cols.length + 1 ? 0 : -1}
                    onFocus={() => setPos({ r: it.ri, c: cols.length + 1 })}
                  >
                    <button
                      className="icon-btn"
                      tabIndex={-1}
                      aria-label={t("Open record")}
                      onClick={() => p.onOpen(it.row.id)}
                    >
                      <Icon name="external" size={14} />
                    </button>
                  </td>
                </tr>
              ),
            )}
          </tbody>
          {cols.some(isAgg) && (
            <tfoot>
              <tr role="row">
                <td colSpan={span} role="gridcell">
                  <strong>
                    {p.pager && p.total > rows.length ? t("Total on this page") : t("Total")}
                  </strong>{" "}
                  <span className="hint">({rows.length})</span>
                </td>
                {cols.slice(firstAgg).map((f) => (
                  <td key={f.name} role="gridcell" className="num agg">
                    {isAgg(f) ? formatSumRows(f, rows) : ""}
                  </td>
                ))}
                <td role="gridcell" />
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
          model={p.model}
          noun={inSentence(p.title)}
          records={selectedRecords}
          onClose={() => setDeleting(false)}
          onDone={() => setSelected(new Set())}
        />
      )}
    </div>
  );
}

const isMaskedVal = (v: unknown) => typeof v === "string" && /^[A-Z][A-Z0-9]*_\d+$/.test(v);
