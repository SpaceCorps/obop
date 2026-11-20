import { useMemo, useState } from "react";
import { fmt, inSentence, t } from "@obop/ui";
import type { FieldMeta, ModelMeta } from "../api.ts";
import { Chart, foldOther, MAX_BARS, OTHER_KEY, type ChartDatum } from "../charts/Chart.tsx";
import { useData } from "../data.tsx";
import { fieldTitle } from "../labels.ts";
import { stateFieldOf, toneClassOf } from "../presentation.ts";
import { domainOf, type Cond, type GraphCfg, type Lookup } from "../query.ts";
import {
  cellValue,
  combine,
  compactMeasure,
  currenciesOf,
  defaultRows,
  dimAxisLabel,
  dimCond,
  dimLabel,
  dimRef,
  dimensionsOf,
  dimTitle,
  fillPeriods,
  formatMeasure,
  isDated,
  measureTitle,
  orderDimKeys,
  parseDim,
  type Cell,
  type Dim,
  type MeasureSel,
  type ReportRow,
} from "../reports.ts";
import { Icon } from "../ui.tsx";
import { useReportRows, useTitleLookup } from "../useAggregate.ts";
import { AggState } from "./AggState.tsx";
import { DimPicker, MeasurePicker, useMeasures } from "./PivotView.tsx";
import type { ViewProps } from "./types.ts";

/** Whether the groups of a dimension carry a meaning that their badge colours show. */
export function tonedDim(model: ModelMeta, f: FieldMeta): boolean {
  return f.kind === "select" && (!!f.ui?.tones || stateFieldOf(model)?.name === f.name);
}

/**
 * The points of a chart from a report's rows in one currency: every option of a select or a
 * yes/no (zero where none), every period of a date between the first and the last, the refs
 * that have records. Labels are short on the axis and full in tooltips.
 */
export function chartPoints(
  model: ModelMeta,
  dim: Dim,
  m: MeasureSel,
  rows: readonly ReportRow[],
  lk: Lookup,
): ChartDatum[] {
  const cells = new Map<string, Cell>();
  for (const r of rows) {
    const k = r.keys[0] ?? "";
    const c = cells.get(k) ?? { count: 0, value: 0 };
    combine(m, c, r);
    cells.set(k, c);
  }
  const present = [...cells.keys()];
  const f = dim.field;
  const keys =
    f.kind === "select" || f.kind === "bool"
      ? domainOf(f, present, lk)
      : isDated(f)
        ? fillPeriods(orderDimKeys(dim, present, lk), dim.bucket ?? "month")
        : orderDimKeys(dim, present, lk);
  const toned = tonedDim(model, f);
  return keys.map((key) => ({
    key,
    label: dimAxisLabel(dim, key, lk),
    full: dimLabel(dim, key, lk),
    value: cellValue(m, cells.get(key)),
    tone: toned ? toneClassOf(f, key, model) : undefined,
  }));
}

/** The condition behind "Other": none of the groups the chart shows. */
export function otherCond(dim: Dim, data: ChartDatum[], keep: number): Cond | null {
  if (dim.field.kind !== "select" && dim.field.kind !== "ref") return null;
  const shown = foldOther(data, keep)
    .filter((x) => x.key !== OTHER_KEY)
    .map((x) => x.key);
  return { op: "isnot", f: dim.field.name, v: shown };
}

export function GraphView(p: ViewProps) {
  const d = useData();
  const { fields, model } = p;
  const cfg: GraphCfg = p.state.graph ?? {};
  const dims = useMemo(() => dimensionsOf(fields), [fields]);
  const { opts, measure: m, index, setup } = useMeasures(model, fields, cfg.measure);
  const dim = parseDim(cfg.dim, dims) ?? defaultRows(model, dims, index, setup);
  const type = cfg.type ?? (dim && isDated(dim.field) ? "line" : "bar");
  const [asTable, setAsTable] = useState(false);
  const lk = useTitleLookup();
  const [pickedCur, setPickedCur] = useState<string | undefined>();
  const set = (patch: Partial<GraphCfg>) => p.setState({ ...p.state, graph: { ...cfg, ...patch } });

  const report = useReportRows({
    server: !!p.server,
    model,
    filter: p.server?.filter ?? [],
    dims: dim ? [dim] : [],
    measure: m,
    rows: p.rows,
  });
  const curs = currenciesOf(report.rows);
  const cur = curs.includes(pickedCur) ? pickedCur : curs[0];
  const dimKey = dim ? dimRef(dim) : "";

  const byCur = useMemo(
    () =>
      dim
        ? curs.map((c) => ({
            cur: c,
            data: chartPoints(
              model,
              dim,
              m,
              report.rows.filter((r) => r.currency === c),
              lk,
            ),
          }))
        : [],
    // The refs stand for the dimension and the currencies; ref labels arrive with `version`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [report.rows, dimKey, curs.join(), model, lk, d.lang, d.locale],
  );

  if (!dim) return <p className="hint pad">{t("Nothing to group these records by.")}</p>;

  const all = byCur.find((x) => x.cur === cur)?.data ?? [];
  // A series in time has no place for records without a date: the table lists them, the chart says so.
  const undated = isDated(dim.field) && all.some((x) => x.key === "" && x.value !== 0);
  const data = isDated(dim.field) ? all.filter((x) => x.key !== "") : all;
  const format = (n: number) => formatMeasure(m, n, cur);
  // "Other" of a smallest or a largest value is the smallest or the largest of the rest, not a sum.
  const merge =
    m?.op === "min"
      ? (v: number[]) => Math.min(...v)
      : m?.op === "max"
        ? (v: number[]) => Math.max(...v)
        : undefined;
  const total = all.reduce((n, x) => n + Math.max(x.value, 0), 0);
  const name = t("{measure} by {dimension}", {
    measure: measureTitle(m),
    dimension: dimTitle(dim),
  });
  const chart =
    type === "pie" ? t("Donut chart") : type === "line" ? t("Line chart") : t("Bar chart");
  const summary = t("{chart}. {name}: {values}.", {
    chart,
    name,
    values: data.map((x) => `${x.full ?? x.label} ${format(x.value)}`).join(", "),
  });
  const keep = type === "pie" ? 7 : 8;
  const pick = (key: string) => {
    if (key !== OTHER_KEY) return p.drill([dimCond(dim, key)]);
    const c = otherCond(dim, data, keep);
    if (c) p.drill([c]);
  };
  const toned = tonedDim(model, dim.field);
  const distinctTones = new Set(data.map((x) => x.tone)).size === data.length;
  const colors =
    toned && (type !== "pie" || distinctTones) ? "tone" : type === "pie" ? "palette" : "single";
  const empty = data.length === 0 || report.rows.length === 0;

  return (
    <div className="graph">
      <div className="viewbar">
        <div className="switcher" role="group" aria-label={t("Chart type")}>
          {(["bar", "line", "pie"] as const).map((k) => (
            <button key={k} aria-pressed={type === k} onClick={() => set({ type: k })}>
              {k === "bar" ? t("Bar") : k === "line" ? t("Line") : t("Pie")}
            </button>
          ))}
        </div>
        <DimPicker
          label={t("Dimension")}
          dims={dims}
          value={dim}
          onChange={(v) => set({ dim: v })}
        />
        <MeasurePicker opts={opts} value={m} onChange={(measure) => set({ measure })} />
        <span className="grow" />
        <button className="btn small" aria-pressed={asTable} onClick={() => setAsTable(!asTable)}>
          <Icon name={asTable ? "graph" : "list"} size={13} />{" "}
          {asTable ? t("Show chart") : t("Show data table")}
        </button>
      </div>
      {report.mixed && (
        <p className="notice-bar" role="status">
          {t(
            "These records use more than one currency; the amounts are added up as if they were one.",
          )}
        </p>
      )}
      <AggState agg={report}>
        {empty ? (
          <p className="hint pad">{t("No data to chart.")}</p>
        ) : asTable ? (
          <div className="tablewrap">
            <table className="table pivot-table">
              <caption className="sr-only">{name}</caption>
              <thead>
                <tr>
                  <th scope="col">{dimTitle(dim)}</th>
                  {byCur.map((x) => (
                    <th key={x.cur ?? ""} scope="col" className="num">
                      {byCur.length > 1 ? `${measureTitle(m)} (${x.cur})` : measureTitle(m)}
                    </th>
                  ))}
                  {byCur.length === 1 && (
                    <th scope="col" className="num">
                      {t("Share")}
                    </th>
                  )}
                </tr>
              </thead>
              <tbody>
                {(byCur[0]?.data ?? []).map((x, i) => (
                  <tr key={x.key || "none"}>
                    <th scope="row">{x.full ?? x.label}</th>
                    {byCur.map((c) => (
                      <td key={c.cur ?? ""} className="num">
                        {formatMeasure(m, c.data[i]?.value ?? 0, c.cur)}
                      </td>
                    ))}
                    {byCur.length === 1 && (
                      <td className="num">
                        {total ? fmt.percent(Math.max(x.value, 0) / total, 0) : "–"}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <figure className="chart">
            <figcaption className="chart-head">
              <span className="chart-title">{name}</span>
              {curs.length > 1 && (
                <span className="switcher small" role="group" aria-label={t("Currency")}>
                  {curs.map((c) => (
                    <button key={c ?? ""} aria-pressed={c === cur} onClick={() => setPickedCur(c)}>
                      {c}
                    </button>
                  ))}
                </span>
              )}
            </figcaption>
            <Chart
              kind={type}
              data={data}
              format={format}
              compact={(n) => compactMeasure(m, n, cur)}
              title={chart}
              summary={summary}
              onPick={pick}
              ordered={isDated(dim.field)}
              colors={colors}
              maxHeight={type === "pie" ? undefined : 360}
              merge={merge}
            />
            {undated && (
              <p className="chart-note">
                {t("Records without {field} are not in the chart; the data table lists them.", {
                  field: inSentence(fieldTitle(dim.field)),
                })}
              </p>
            )}
            {data.length > MAX_BARS && type === "bar" && !isDated(dim.field) && (
              <p className="chart-note">
                {m?.op === "min"
                  ? t(
                      "The {count} largest are shown; Other shows the smallest value of the rest.",
                      {
                        count: keep,
                      },
                    )
                  : m?.op === "max"
                    ? t(
                        "The {count} largest are shown; Other shows the largest value of the rest.",
                        { count: keep },
                      )
                    : t("The {count} largest are shown; the rest are added up as Other.", {
                        count: keep,
                      })}
              </p>
            )}
          </figure>
        )}
      </AggState>
    </div>
  );
}
