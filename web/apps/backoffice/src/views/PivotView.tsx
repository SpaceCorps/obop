import { useMemo } from "react";
import { t } from "@obop/ui";
import type { AggGroup, FieldMeta, Row } from "../api.ts";
import { useData } from "../data.tsx";
import { fieldTitle } from "../labels.ts";
import {
  condForKey,
  currencyOf,
  formatSum,
  groupKey,
  groupLabel,
  groupable,
  measurable,
  orderKeys,
  type PivotCfg,
} from "../query.ts";
import {
  groupByFor,
  keyOf,
  measuresFor,
  useAggregate,
  useCurrencyOf,
  valueOfGroup,
} from "../useAggregate.ts";
import { AggState } from "./AggState.tsx";
import { kanbanField } from "./KanbanView.tsx";
import type { ViewProps } from "./types.ts";

export function measureOf(fields: FieldMeta[], name: string | undefined) {
  return measurable(fields).find((f) => f.name === name);
}

export function value(rows: Row[], m: FieldMeta | undefined): number {
  return m ? rows.reduce((n, r) => n + (Number(r[m.name]) || 0), 0) : rows.length;
}

export const fmt = (m: FieldMeta | undefined, n: number, currency?: string) =>
  formatSum(m, n, currency);

/** The English-free name of a measure: "Count" or "Sum of Expected revenue". */
export const measureName = (m: FieldMeta | undefined) =>
  m ? t("Sum of {field}", { field: fieldTitle(m) }) : t("Count");

export function MeasurePicker(props: {
  fields: FieldMeta[];
  value: string | undefined;
  onChange: (v: string) => void;
}) {
  return (
    <label className="inline-field">
      <span>{t("Measure")}</span>
      <select value={props.value ?? "count"} onChange={(e) => props.onChange(e.target.value)}>
        <option value="count">{t("Count")}</option>
        {measurable(props.fields).map((f) => (
          <option key={f.name} value={f.name}>
            {t("Sum of {field}", { field: fieldTitle(f) })}
          </option>
        ))}
      </select>
    </label>
  );
}

export type Cell = { count: number; value: number };
const cellKey = (r: string, c: string) => `${r}\u0001${c}`;

/** Cells of the pivot from records in the browser (the fallback for a small model). */
export function clientCells(
  rows: Row[],
  rowF: FieldMeta,
  colF: FieldMeta | undefined,
  m: FieldMeta | undefined,
): Map<string, Cell> {
  const cells = new Map<string, Cell>();
  for (const r of rows) {
    const k = cellKey(groupKey(rowF, r), colF ? groupKey(colF, r) : "");
    const c = cells.get(k) ?? { count: 0, value: 0 };
    c.count += 1;
    c.value += m ? Number(r[m.name]) || 0 : 1;
    cells.set(k, c);
  }
  return cells;
}

/** Cells of the pivot from the server's groups (D4). */
export function serverCells(
  groups: AggGroup[],
  rowF: FieldMeta,
  colF: FieldMeta | undefined,
  m: FieldMeta | undefined,
): Map<string, Cell> {
  const cells = new Map<string, Cell>();
  for (const g of groups)
    cells.set(cellKey(keyOf(g.key[rowF.name]), colF ? keyOf(g.key[colF.name]) : ""), {
      count: g.count,
      value: valueOfGroup(g, m),
    });
  return cells;
}

export function PivotView(p: ViewProps) {
  const d = useData();
  const { fields, rows } = p;
  const cfg: PivotCfg = p.state.pivot ?? {};
  const gfs = groupable(fields);
  const rowF = gfs.find((f) => f.name === cfg.row) ?? kanbanField(fields, undefined)!;
  const colF = gfs.find((f) => f.name === cfg.col && f.name !== rowF.name);
  const m = measureOf(fields, cfg.measure);
  const set = (patch: Partial<PivotCfg>) => p.setState({ ...p.state, pivot: { ...cfg, ...patch } });

  const agg = useAggregate({
    enabled: !!p.server,
    model: p.model.name,
    filter: p.server?.filter ?? [],
    groupBy: [groupByFor(rowF), ...(colF ? [groupByFor(colF)] : [])],
    measures: measuresFor(m),
  });
  const money = useCurrencyOf(!!p.server, p.model, p.server?.filter ?? []);

  const data = useMemo(() => {
    const cells = p.server
      ? serverCells(agg.groups, rowF, colF, m)
      : clientCells(rows, rowF, colF, m);
    const rks = new Set<string>();
    const cks = new Set<string>();
    for (const k of cells.keys()) {
      const [r, c] = k.split("\u0001") as [string, string];
      rks.add(r);
      cks.add(c);
    }
    const rk = orderKeys(rowF, [...rks], d.lookup);
    const ck = colF ? orderKeys(colF, [...cks], d.lookup) : [""];
    // Sums and counts add up, so the margins come from the cells.
    const rowTotal = new Map<string, number>();
    const colTotal = new Map<string, number>();
    let grand = 0;
    for (const [k, c] of cells) {
      const [r, cc] = k.split("\u0001") as [string, string];
      rowTotal.set(r, (rowTotal.get(r) ?? 0) + c.value);
      colTotal.set(cc, (colTotal.get(cc) ?? 0) + c.value);
      grand += c.value;
    }
    return { rk, ck, cells, rowTotal, colTotal, grand };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.server, agg.groups, rows, rowF, colF, m, d.version, d.lang]);

  const drill = (rk: string | null, ck: string | null) =>
    p.drill([
      ...(rk === null ? [] : [condForKey(rowF, rk)]),
      ...(ck === null || !colF ? [] : [condForKey(colF, ck)]),
    ]);
  const cur = p.server ? money : currencyOf(rows);
  const show = (n: number) => (n === 0 ? <span className="dim">–</span> : fmt(m, n, cur.currency));

  return (
    <div className="pivot">
      <div className="viewbar">
        <label className="inline-field">
          <span>{t("Rows")}</span>
          <select value={rowF.name} onChange={(e) => set({ row: e.target.value })}>
            {gfs.map((f) => (
              <option key={f.name} value={f.name}>
                {f.kind === "date" ? t("{field} (month)", { field: fieldTitle(f) }) : fieldTitle(f)}
              </option>
            ))}
          </select>
        </label>
        <label className="inline-field">
          <span>{t("Columns")}</span>
          <select
            value={colF?.name ?? ""}
            onChange={(e) => set({ col: e.target.value || undefined })}
          >
            <option value="">{t("None")}</option>
            {gfs
              .filter((f) => f.name !== rowF.name)
              .map((f) => (
                <option key={f.name} value={f.name}>
                  {f.kind === "date"
                    ? t("{field} (month)", { field: fieldTitle(f) })
                    : fieldTitle(f)}
                </option>
              ))}
          </select>
        </label>
        <MeasurePicker
          fields={fields}
          value={m?.name}
          onChange={(v) => set({ measure: v === "count" ? undefined : v })}
        />
      </div>
      {cur.mixed && (
        <p className="notice-bar" role="status">
          {t(
            "These records use more than one currency; the amounts are added up as if they were one.",
          )}
        </p>
      )}
      <AggState agg={p.server ? agg : { ...agg, loaded: true, error: undefined }}>
        <div className="tablewrap">
          <table className="table pivot-table">
            <caption className="sr-only">
              {colF
                ? t("{measure} by {rows} and {columns}. Select a cell to list its records.", {
                    measure: measureName(m),
                    rows: fieldTitle(rowF),
                    columns: fieldTitle(colF),
                  })
                : t("{measure} by {rows}. Select a cell to list its records.", {
                    measure: measureName(m),
                    rows: fieldTitle(rowF),
                  })}
            </caption>
            <thead>
              <tr>
                <th scope="col">{fieldTitle(rowF)}</th>
                {colF
                  ? data.ck.map((c) => (
                      <th key={c} scope="col" className="num">
                        {groupLabel(colF, c, d.lookup)}
                      </th>
                    ))
                  : null}
                <th scope="col" className="num total">
                  {measureName(m)}
                </th>
              </tr>
            </thead>
            <tbody>
              {data.rk.map((r) => (
                <tr key={r || "none"}>
                  <th scope="row">
                    <button className="cellbtn" onClick={() => drill(r, null)}>
                      {groupLabel(rowF, r, d.lookup)}
                    </button>
                  </th>
                  {colF &&
                    data.ck.map((c) => {
                      const cell = data.cells.get(cellKey(r, c));
                      return (
                        <td key={c} className="num">
                          {cell?.count ? (
                            <button
                              className="cellbtn"
                              aria-label={t("{row}, {column}: {value}", {
                                row: groupLabel(rowF, r, d.lookup),
                                column: groupLabel(colF, c, d.lookup),
                                value: fmt(m, cell.value, cur.currency),
                              })}
                              onClick={() => drill(r, c)}
                            >
                              {show(cell.value)}
                            </button>
                          ) : (
                            <span className="dim">–</span>
                          )}
                        </td>
                      );
                    })}
                  <td className="num total">
                    <button className="cellbtn" onClick={() => drill(r, null)}>
                      {show(data.rowTotal.get(r) ?? 0)}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th scope="row">{t("Total")}</th>
                {colF &&
                  data.ck.map((c) => (
                    <td key={c} className="num total">
                      <button className="cellbtn" onClick={() => drill(null, c)}>
                        {show(data.colTotal.get(c) ?? 0)}
                      </button>
                    </td>
                  ))}
                <td className="num total">
                  <button className="cellbtn" onClick={() => drill(null, null)}>
                    {show(data.grand)}
                  </button>
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      </AggState>
    </div>
  );
}
