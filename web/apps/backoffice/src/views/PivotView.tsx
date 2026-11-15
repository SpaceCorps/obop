import { Fragment, useMemo, useState } from "react";
import { t } from "@obop/ui";
import type { FieldMeta, ModelMeta } from "../api.ts";
import { useData } from "../data.tsx";
import { fieldTitle } from "../labels.ts";
import { indexModels, menuOf, type MetaIndex } from "../presentation.ts";
import type { PivotCfg } from "../query.ts";
import {
  BUCKETS,
  bucketName,
  cellValue,
  defaultCols,
  defaultMeasure,
  defaultRows,
  dimCond,
  dimLabel,
  dimRef,
  dimensionsOf,
  dimTitle,
  formatMeasure,
  isDated,
  matrixOf,
  measureOptions,
  measureRef,
  measureTitle,
  parseDim,
  parseMeasure,
  type Bucket,
  type Cell,
  type Dim,
  type MeasureOptions,
  type MeasureSel,
} from "../reports.ts";
import { IconButton } from "../ui.tsx";
import { useReportRows, useTitleLookup } from "../useAggregate.ts";
import { AggState } from "./AggState.tsx";
import type { ViewProps } from "./types.ts";

/** Whether a model is set-up data of its app (a tax, a plan), which only counts. */
export function isConfiguration(
  model: ModelMeta,
  index: MetaIndex,
  models: ReturnType<typeof useData>["models"],
): boolean {
  const mod = models.get(model.name)?.module;
  return !!mod && menuOf(mod, index).configuration.some((m) => m.name === model.name);
}

/** All models by name, and which of them are set-up data, for the defaults of a report. */
export function useReportIndex(): { index: MetaIndex; setup: (model: string) => boolean } {
  const d = useData();
  return useMemo(() => {
    const index = indexModels(d.modules);
    const setup = (name: string) => {
      const m = d.models.get(name)?.model;
      return !!m && isConfiguration(m, index, d.models);
    };
    return { index, setup };
  }, [d.modules, d.models]);
}

/**
 * The measures a model offers, and the one it opens with; shared by the pivot and the graph so
 * they agree. `ref` is the measure of the address (`count`, `field`, `avg:field`).
 */
export function useMeasures(
  model: ModelMeta,
  fields: FieldMeta[],
  ref: string | undefined,
): {
  opts: MeasureOptions;
  measure: MeasureSel;
  index: MetaIndex;
  setup: (model: string) => boolean;
} {
  const { index, setup } = useReportIndex();
  const opts = useMemo(
    () => measureOptions(model, fields, setup(model.name)),
    [model, fields, setup],
  );
  const read = parseMeasure(ref, opts);
  const measure = read === undefined ? defaultMeasure(model, opts, fields) : read;
  return { opts, measure, index, setup };
}

/** The measure menu: the count, the sums, then the averages. */
export function MeasurePicker(props: {
  opts: MeasureOptions;
  value: MeasureSel;
  onChange: (ref: string) => void;
}) {
  const { sums, avgs } = props.opts;
  return (
    <label className="inline-field">
      <span>{t("Measure")}</span>
      <select value={measureRef(props.value)} onChange={(e) => props.onChange(e.target.value)}>
        <option value="count">{t("Count")}</option>
        {sums.length > 0 && (
          <optgroup label={t("Sum")}>
            {sums.map((f) => (
              <option key={f.name} value={f.name}>
                {measureTitle({ field: f, op: "sum" })}
              </option>
            ))}
          </optgroup>
        )}
        {avgs.length > 0 && (
          <optgroup label={t("Average")}>
            {avgs.map((f) => (
              <option key={f.name} value={`avg:${f.name}`}>
                {measureTitle({ field: f, op: "avg" })}
              </option>
            ))}
          </optgroup>
        )}
      </select>
    </label>
  );
}

/** A dimension menu with, for a date, the period beside it. */
export function DimPicker(props: {
  label: string;
  dims: FieldMeta[];
  value: Dim | undefined;
  /** The text of the empty choice; no empty choice when absent. */
  none?: string;
  className?: string;
  onChange: (ref: string) => void;
}) {
  const v = props.value;
  return (
    <span className={`dimpick${props.className ? ` ${props.className}` : ""}`}>
      <label className="inline-field">
        <span>{props.label}</span>
        <select
          value={v?.field.name ?? ""}
          onChange={(e) => {
            const f = props.dims.find((x) => x.name === e.target.value);
            // A new date keeps the period chosen for the old one.
            props.onChange(
              f && isDated(f) && v?.bucket
                ? dimRef({ field: f, bucket: v.bucket })
                : e.target.value,
            );
          }}
        >
          {props.none !== undefined && <option value="">{props.none}</option>}
          {props.dims.map((f) => (
            <option key={f.name} value={f.name}>
              {fieldTitle(f)}
            </option>
          ))}
        </select>
      </label>
      {v && isDated(v.field) && (
        <select
          className="period"
          aria-label={t("Period of {field}", { field: fieldTitle(v.field) })}
          value={v.bucket ?? "month"}
          onChange={(e) =>
            props.onChange(dimRef({ field: v.field, bucket: e.target.value as Bucket }))
          }
        >
          {BUCKETS.map((b) => (
            <option key={b} value={b}>
              {bucketName(b)}
            </option>
          ))}
        </select>
      )}
    </span>
  );
}

type Sort = "desc" | "asc" | null;
const TOTAL = "\u0000total";

export function PivotView(p: ViewProps) {
  const d = useData();
  const { fields, model } = p;
  const cfg: PivotCfg = p.state.pivot ?? {};
  const dims = useMemo(() => dimensionsOf(fields), [fields]);
  const { opts, measure: m, index, setup } = useMeasures(model, fields, cfg.measure);
  const rowD = parseDim(cfg.row, dims) ?? defaultRows(model, dims, index, setup);
  const colPick =
    cfg.col === "" ? undefined : (parseDim(cfg.col, dims) ?? defaultCols(model, dims));
  const colD = colPick && rowD && colPick.field.name !== rowD.field.name ? colPick : undefined;
  const [sort, setSort] = useState<Sort>(null);
  const lk = useTitleLookup();
  const set = (patch: Partial<PivotCfg>) => p.setState({ ...p.state, pivot: { ...cfg, ...patch } });

  const report = useReportRows({
    server: !!p.server,
    model,
    filter: p.server?.filter ?? [],
    dims: rowD ? (colD ? [rowD, colD] : [rowD]) : [],
    measure: m,
    rows: p.rows,
  });

  const rowRef = rowD ? dimRef(rowD) : "";
  const colRef = colD ? dimRef(colD) : "";
  const mx = useMemo(
    () => (rowD ? matrixOf(report.rows, [rowD, colD], lk) : null),
    // The refs stand for the dimensions; ref labels arrive with `version`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [report.rows, rowRef, colRef, lk, d.lang],
  );

  if (!rowD || !mx) return <p className="hint pad">{t("Nothing to group these records by.")}</p>;

  const curs = mx.currencies;
  const split = curs.length > 1;
  const value = (c: Cell | undefined) => cellValue(m, c);
  const show = (c: Cell | undefined, cur: string | undefined) =>
    !c?.count ? <span className="dim">–</span> : formatMeasure(m, value(c), cur);
  const rowKeys =
    sort === null
      ? mx.rowKeys
      : [...mx.rowKeys].sort((a, b) => {
          const va = value(mx.rowTotal(a, curs[0]));
          const vb = value(mx.rowTotal(b, curs[0]));
          return sort === "desc" ? vb - va : va - vb;
        });
  const drill = (rk: string | null, ck: string | null) =>
    p.drill([
      ...(rk === null ? [] : [dimCond(rowD, rk)]),
      ...(ck === null || !colD ? [] : [dimCond(colD, ck)]),
    ]);
  const rowLabel = (k: string) => dimLabel(rowD, k, lk);
  const colLabel = (k: string) => (colD ? dimLabel(colD, k, lk) : "");
  const nextSort = (): Sort => (sort === null ? "desc" : sort === "desc" ? "asc" : null);
  const ariaSort = sort === "desc" ? "descending" : sort === "asc" ? "ascending" : undefined;

  return (
    <div className="pivot">
      <div className="viewbar">
        <DimPicker label={t("Rows")} dims={dims} value={rowD} onChange={(row) => set({ row })} />
        <DimPicker
          label={t("Columns")}
          className="cols"
          dims={dims.filter((f) => f.name !== rowD.field.name)}
          value={colD}
          none={t("None")}
          onChange={(col) => set({ col })}
        />
        {colD && (
          <IconButton
            icon="sort"
            size="sm"
            label={t("Flip axes")}
            onClick={() => set({ row: dimRef(colD), col: dimRef(rowD) })}
          />
        )}
        <MeasurePicker opts={opts} value={m} onChange={(measure) => set({ measure })} />
      </div>
      {report.mixed && (
        <p className="notice-bar" role="status">
          {t(
            "These records use more than one currency; the amounts are added up as if they were one.",
          )}
        </p>
      )}
      <AggState agg={report}>
        {mx.rowKeys.length === 0 ? (
          <p className="hint pad">{t("No records to add up.")}</p>
        ) : (
          <div className="tablewrap">
            <table className="table pivot-table">
              <caption className="sr-only">
                {colD
                  ? t("{measure} by {rows} and {columns}. Select a cell to list its records.", {
                      measure: measureTitle(m),
                      rows: dimTitle(rowD),
                      columns: dimTitle(colD),
                    })
                  : t("{measure} by {rows}. Select a cell to list its records.", {
                      measure: measureTitle(m),
                      rows: dimTitle(rowD),
                    })}
              </caption>
              <thead>
                <tr>
                  <th scope="col" rowSpan={split ? 2 : 1}>
                    {dimTitle(rowD)}
                  </th>
                  {colD &&
                    mx.colKeys.map((c) => (
                      <th
                        key={c || "none"}
                        scope={split ? "colgroup" : "col"}
                        colSpan={curs.length}
                        className="num"
                      >
                        {colLabel(c)}
                      </th>
                    ))}
                  <th
                    scope={split ? "colgroup" : "col"}
                    colSpan={curs.length}
                    className="num total"
                    aria-sort={ariaSort}
                  >
                    <button
                      type="button"
                      className="sortbtn"
                      onClick={() => setSort(nextSort())}
                      title={t("Sort the rows by this total")}
                    >
                      {colD ? t("Total") : measureTitle(m)}
                      {sort && (
                        <span className="sortmark" aria-hidden="true">
                          {sort === "desc" ? "↓" : "↑"}
                        </span>
                      )}
                    </button>
                  </th>
                </tr>
                {split && (
                  <tr className="curs">
                    {[...(colD ? mx.colKeys : []), TOTAL].map((c) =>
                      curs.map((cur) => (
                        <th
                          key={`${c}-${cur ?? ""}`}
                          scope="col"
                          className={`num${c === TOTAL ? " total" : ""}`}
                        >
                          {cur}
                        </th>
                      )),
                    )}
                  </tr>
                )}
              </thead>
              <tbody>
                {rowKeys.map((r) => (
                  <tr key={r || "none"}>
                    <th scope="row">
                      <button className="cellbtn" onClick={() => drill(r, null)}>
                        {rowLabel(r)}
                      </button>
                    </th>
                    {colD &&
                      mx.colKeys.map((c) =>
                        curs.map((cur) => {
                          const cell = mx.cell(r, c, cur);
                          return (
                            <td key={`${c}-${cur ?? ""}`} className="num">
                              {cell?.count ? (
                                <button
                                  className="cellbtn"
                                  aria-label={t("{row}, {column}: {value}", {
                                    row: rowLabel(r),
                                    column: colLabel(c),
                                    value: formatMeasure(m, value(cell), cur),
                                  })}
                                  onClick={() => drill(r, c)}
                                >
                                  {show(cell, cur)}
                                </button>
                              ) : (
                                <span className="dim">–</span>
                              )}
                            </td>
                          );
                        }),
                      )}
                    {curs.map((cur) => (
                      <td key={cur ?? ""} className="num total">
                        <button className="cellbtn" onClick={() => drill(r, null)}>
                          {show(mx.rowTotal(r, cur), cur)}
                        </button>
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">{t("Total")}</th>
                  {colD &&
                    mx.colKeys.map((c) => (
                      <Fragment key={c || "none"}>
                        {curs.map((cur) => (
                          <td key={cur ?? ""} className="num total">
                            <button className="cellbtn" onClick={() => drill(null, c)}>
                              {show(mx.colTotal(c, cur), cur)}
                            </button>
                          </td>
                        ))}
                      </Fragment>
                    ))}
                  {curs.map((cur) => (
                    <td key={cur ?? ""} className="num total">
                      <button className="cellbtn" onClick={() => drill(null, null)}>
                        {show(mx.grand(cur), cur)}
                      </button>
                    </td>
                  ))}
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </AggState>
    </div>
  );
}
