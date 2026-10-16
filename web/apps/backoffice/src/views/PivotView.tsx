import { useMemo } from "react";
import type { FieldMeta, Row } from "../api.ts";
import { useData } from "../data.tsx";
import { label } from "../format.ts";
import {
  condForKey,
  formatSum,
  groupKey,
  groupLabel,
  groupable,
  measurable,
  orderKeys,
  type PivotCfg,
} from "../query.ts";
import { kanbanField } from "./KanbanView.tsx";
import type { ViewProps } from "./types.ts";

export function measureOf(fields: FieldMeta[], name: string | undefined) {
  return measurable(fields).find((f) => f.name === name);
}

export function value(rows: Row[], m: FieldMeta | undefined): number {
  return m ? rows.reduce((n, r) => n + (Number(r[m.name]) || 0), 0) : rows.length;
}

export const fmt = (m: FieldMeta | undefined, n: number) => formatSum(m, n);

export function MeasurePicker(props: {
  fields: FieldMeta[];
  value: string | undefined;
  onChange: (v: string) => void;
}) {
  return (
    <label className="inline-field">
      <span>Measure</span>
      <select value={props.value ?? "count"} onChange={(e) => props.onChange(e.target.value)}>
        <option value="count">Count</option>
        {measurable(props.fields).map((f) => (
          <option key={f.name} value={f.name}>
            Sum of {label(f.name)}
          </option>
        ))}
      </select>
    </label>
  );
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

  const data = useMemo(() => {
    const rk = orderKeys(
      rowF,
      rows.map((r) => groupKey(rowF, r)),
      d.lookup,
    );
    const ck = colF
      ? orderKeys(
          colF,
          rows.map((r) => groupKey(colF, r)),
          d.lookup,
        )
      : [""];
    const cell = new Map<string, Row[]>();
    for (const r of rows) {
      const k = `${groupKey(rowF, r)}\u0001${colF ? groupKey(colF, r) : ""}`;
      const a = cell.get(k);
      if (a) a.push(r);
      else cell.set(k, [r]);
    }
    return { rk, ck, cell };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, rowF, colF, d.version]);

  const rowsOf = (rk: string | null, ck: string | null) =>
    rows.filter(
      (r) =>
        (rk === null || groupKey(rowF, r) === rk) &&
        (ck === null || !colF || groupKey(colF, r) === ck),
    );
  const drill = (rk: string | null, ck: string | null) =>
    p.drill([
      ...(rk === null ? [] : [condForKey(rowF, rk)]),
      ...(ck === null || !colF ? [] : [condForKey(colF, ck)]),
    ]);
  const show = (n: number) => (n === 0 ? <span className="dim">–</span> : fmt(m, n));

  return (
    <div className="pivot">
      <div className="viewbar">
        <label className="inline-field">
          <span>Rows</span>
          <select value={rowF.name} onChange={(e) => set({ row: e.target.value })}>
            {gfs.map((f) => (
              <option key={f.name} value={f.name}>
                {label(f.name)}
                {f.kind === "date" ? " (month)" : ""}
              </option>
            ))}
          </select>
        </label>
        <label className="inline-field">
          <span>Columns</span>
          <select
            value={colF?.name ?? ""}
            onChange={(e) => set({ col: e.target.value || undefined })}
          >
            <option value="">None</option>
            {gfs
              .filter((f) => f.name !== rowF.name)
              .map((f) => (
                <option key={f.name} value={f.name}>
                  {label(f.name)}
                  {f.kind === "date" ? " (month)" : ""}
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
      <div className="tablewrap">
        <table className="pivot-table">
          <caption className="sr-only">
            {m ? `Sum of ${label(m.name)}` : "Count"} by {label(rowF.name)}
            {colF ? ` and ${label(colF.name)}` : ""}. Select a cell to list its records.
          </caption>
          <thead>
            <tr>
              <th scope="col">{label(rowF.name)}</th>
              {colF
                ? data.ck.map((c) => (
                    <th key={c} scope="col" className="num">
                      {groupLabel(colF, c, d.lookup)}
                    </th>
                  ))
                : null}
              <th scope="col" className="num total">
                {m ? `Sum of ${label(m.name)}` : "Count"}
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
                    const cell = data.cell.get(`${r}\u0001${c}`) ?? [];
                    return (
                      <td key={c} className="num">
                        {cell.length ? (
                          <button
                            className="cellbtn"
                            aria-label={`${groupLabel(rowF, r, d.lookup)}, ${groupLabel(colF, c, d.lookup)}: ${fmt(m, value(cell, m))}`}
                            onClick={() => drill(r, c)}
                          >
                            {show(value(cell, m))}
                          </button>
                        ) : (
                          <span className="dim">–</span>
                        )}
                      </td>
                    );
                  })}
                <td className="num total">
                  <button className="cellbtn" onClick={() => drill(r, null)}>
                    {show(value(rowsOf(r, null), m))}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row">Total</th>
              {colF &&
                data.ck.map((c) => (
                  <td key={c} className="num total">
                    <button className="cellbtn" onClick={() => drill(null, c)}>
                      {show(value(rowsOf(null, c), m))}
                    </button>
                  </td>
                ))}
              <td className="num total">
                <button className="cellbtn" onClick={() => drill(null, null)}>
                  {show(value(rows, m))}
                </button>
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
    </div>
  );
}
