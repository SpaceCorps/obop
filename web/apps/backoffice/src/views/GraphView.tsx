import { useId, useMemo, useState } from "react";
import type { FieldMeta } from "../api.ts";
import { useData } from "../data.tsx";
import { label } from "../format.ts";
import {
  condForKey,
  groupDomain,
  groupKey,
  groupLabel,
  groupable,
  orderKeys,
  type GraphCfg,
} from "../query.ts";
import { Icon } from "../ui.tsx";
import { fmt, MeasurePicker, measureOf, value } from "./PivotView.tsx";
import { kanbanField } from "./KanbanView.tsx";
import type { ViewProps } from "./types.ts";

export type Datum = { key: string; label: string; value: number };

const W = 640;
const H = 340;
const M = { l: 64, r: 16, t: 20, b: 64 };
const color = (i: number) => `var(--chart-${(i % 8) + 1})`;

const compact = (m: FieldMeta | undefined, n: number) =>
  m?.kind === "money"
    ? new Intl.NumberFormat(undefined, {
        style: "currency",
        currency: "EUR",
        notation: "compact",
        maximumFractionDigits: 1,
      }).format(n / 100)
    : new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(n);

/** A "nice" axis maximum and tick step for the largest value. */
function niceScale(max: number, step = 4): { top: number; ticks: number[] } {
  if (max <= 0) return { top: 1, ticks: [0, 1] };
  const raw = max / step;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const unit = [1, 2, 2.5, 5, 10].map((k) => k * pow).find((u) => u >= raw) ?? raw;
  const top = Math.ceil(max / unit) * unit;
  const ticks: number[] = [];
  for (let t = 0; t <= top + unit / 2; t += unit) ticks.push(t);
  return { top, ticks };
}

export function GraphView(p: ViewProps) {
  const d = useData();
  const { fields, rows } = p;
  const cfg: GraphCfg = p.state.graph ?? {};
  const gfs = groupable(fields);
  const dim = gfs.find((f) => f.name === cfg.dim) ?? kanbanField(fields, undefined)!;
  const m = measureOf(fields, cfg.measure);
  const type = cfg.type ?? (dim.kind === "date" ? "line" : "bar");
  const [asTable, setAsTable] = useState(false);
  const id = useId();
  const set = (patch: Partial<GraphCfg>) => p.setState({ ...p.state, graph: { ...cfg, ...patch } });

  const data = useMemo<Datum[]>(() => {
    const keys =
      dim.kind === "select" || dim.kind === "bool"
        ? groupDomain(dim, rows, d.lookup)
        : orderKeys(
            dim,
            rows.map((r) => groupKey(dim, r)),
            d.lookup,
          );
    return keys.map((key) => ({
      key,
      label: groupLabel(dim, key, d.lookup),
      value: value(
        rows.filter((r) => groupKey(dim, r) === key),
        m,
      ),
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, dim, m, d.version]);

  const total = data.reduce((n, x) => n + Math.max(x.value, 0), 0);
  const name = `${m ? `Sum of ${label(m.name)}` : "Count"} by ${label(dim.name)}`;
  const summary = `${type === "pie" ? "Donut" : type === "line" ? "Line" : "Bar"} chart. ${name}: ${data
    .map((x) => `${x.label} ${fmt(m, x.value)}`)
    .join(", ")}.`;
  const drill = (key: string) => p.drill([condForKey(dim, key)]);

  return (
    <div className="graph">
      <div className="viewbar">
        <div className="switcher" role="group" aria-label="Chart type">
          {(["bar", "line", "pie"] as const).map((t) => (
            <button key={t} aria-pressed={type === t} onClick={() => set({ type: t })}>
              {t === "bar" ? "Bar" : t === "line" ? "Line" : "Pie"}
            </button>
          ))}
        </div>
        <label className="inline-field">
          <span>Dimension</span>
          <select value={dim.name} onChange={(e) => set({ dim: e.target.value })}>
            {gfs.map((f) => (
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
        <span className="grow" />
        <button className="btn small" aria-pressed={asTable} onClick={() => setAsTable(!asTable)}>
          <Icon name={asTable ? "graph" : "list"} size={13} />{" "}
          {asTable ? "Show chart" : "Show data table"}
        </button>
      </div>
      {data.length === 0 ? (
        <p className="hint pad">No data to chart.</p>
      ) : asTable ? (
        <div className="tablewrap">
          <table className="table pivot-table">
            <caption className="sr-only">{name}</caption>
            <thead>
              <tr>
                <th scope="col">{label(dim.name)}</th>
                <th scope="col" className="num">
                  {m ? `Sum of ${label(m.name)}` : "Count"}
                </th>
                <th scope="col" className="num">
                  Share
                </th>
              </tr>
            </thead>
            <tbody>
              {data.map((x) => (
                <tr key={x.key || "none"}>
                  <th scope="row">{x.label}</th>
                  <td className="num">{fmt(m, x.value)}</td>
                  <td className="num">
                    {total ? `${Math.round((Math.max(x.value, 0) / total) * 100)}%` : "–"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <figure className="chart">
          <figcaption className="chart-title">{name}</figcaption>
          {type === "pie" ? (
            <Pie data={data} m={m} total={total} id={id} summary={summary} onPick={drill} />
          ) : (
            <Cartesian data={data} m={m} type={type} id={id} summary={summary} onPick={drill} />
          )}
        </figure>
      )}
    </div>
  );
}

export type ChartProps = {
  data: Datum[];
  m: FieldMeta | undefined;
  id: string;
  summary: string;
  onPick: (key: string) => void;
};

const activate = (fn: () => void) => ({
  role: "button" as const,
  tabIndex: 0,
  onClick: fn,
  onKeyDown: (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      fn();
    }
  },
});

export function Cartesian({
  data,
  m,
  type,
  id,
  summary,
  onPick,
}: ChartProps & { type: "bar" | "line" }) {
  const max = Math.max(...data.map((x) => x.value), 0);
  const { top, ticks } = niceScale(max);
  const iw = W - M.l - M.r;
  const ih = H - M.t - M.b;
  const band = iw / data.length;
  const y = (v: number) => M.t + ih - (Math.max(v, 0) / top) * ih;
  const cx = (i: number) => M.l + band * i + band / 2;
  const rotate = data.length > 6;
  const trunc = (s: string) => (s.length > 14 ? `${s.slice(0, 13)}…` : s);
  const pts = data.map((x, i) => `${cx(i)},${y(x.value)}`).join(" ");
  const mark = (x: Datum) => `${x.label}: ${fmt(m, x.value)}. Show records.`;
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className="chartsvg"
      role="group"
      aria-labelledby={`${id}-t`}
      aria-describedby={`${id}-d`}
    >
      <title id={`${id}-t`}>{summary.split(".")[0]}</title>
      <desc id={`${id}-d`}>{summary}</desc>
      {ticks.map((t) => (
        <g key={t}>
          <line x1={M.l} x2={W - M.r} y1={y(t)} y2={y(t)} className="grid" />
          <text x={M.l - 8} y={y(t)} className="axis" textAnchor="end" dominantBaseline="middle">
            {compact(m, t)}
          </text>
        </g>
      ))}
      {type === "line" && (
        <>
          <polygon
            points={`${cx(0)},${y(0)} ${pts} ${cx(data.length - 1)},${y(0)}`}
            className="area"
          />
          <polyline points={pts} className="line" />
        </>
      )}
      {data.map((x, i) => (
        <g key={x.key || "none"}>
          {type === "bar" ? (
            <g {...activate(() => onPick(x.key))} aria-label={mark(x)} className="mark">
              <title>{`${x.label}: ${fmt(m, x.value)}`}</title>
              <rect
                x={cx(i) - Math.min(band * 0.36, 44)}
                width={Math.min(band * 0.72, 88)}
                y={y(x.value)}
                height={Math.max(y(0) - y(x.value), 0)}
                rx={5}
                style={{ fill: color(i) }}
              />
            </g>
          ) : (
            <g {...activate(() => onPick(x.key))} aria-label={mark(x)} className="mark">
              <title>{`${x.label}: ${fmt(m, x.value)}`}</title>
              <circle cx={cx(i)} cy={y(x.value)} r={5} className="dot" />
            </g>
          )}
          {data.length <= 12 && (
            <text x={cx(i)} y={y(x.value) - 9} textAnchor="middle" className="vlabel-t">
              {compact(m, x.value)}
            </text>
          )}
          <text
            x={cx(i)}
            y={H - M.b + 18}
            className="axis"
            textAnchor={rotate ? "end" : "middle"}
            transform={rotate ? `rotate(-35 ${cx(i)} ${H - M.b + 18})` : undefined}
          >
            {trunc(x.label)}
          </text>
        </g>
      ))}
    </svg>
  );
}

function arc(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  const p = (r: number, a: number) => `${cx + r * Math.cos(a)} ${cy + r * Math.sin(a)}`;
  const large = a1 - a0 > Math.PI ? 1 : 0;
  return `M ${p(r1, a0)} A ${r1} ${r1} 0 ${large} 1 ${p(r1, a1)} L ${p(r0, a1)} A ${r0} ${r0} 0 ${large} 0 ${p(r0, a0)} Z`;
}

export function Pie({ data, m, total, id, summary, onPick }: ChartProps & { total: number }) {
  const slices = data.map((x, i) => ({ ...x, i })).filter((x) => x.value > 0);
  let angle = -Math.PI / 2;
  return (
    <div className="piewrap">
      <svg
        viewBox="0 0 300 300"
        className="piesvg"
        role="group"
        aria-labelledby={`${id}-t`}
        aria-describedby={`${id}-d`}
      >
        <title id={`${id}-t`}>{summary.split(".")[0]}</title>
        <desc id={`${id}-d`}>{summary}</desc>
        {slices.map((s) => {
          const share = s.value / total;
          const a0 = angle;
          angle += share * Math.PI * 2;
          const label = `${s.label}: ${fmt(m, s.value)}, ${Math.round(share * 100)}%. Show records.`;
          return (
            <g
              key={s.key || "none"}
              {...activate(() => onPick(s.key))}
              aria-label={label}
              className="mark"
            >
              <title>{label.replace(" Show records.", "")}</title>
              {slices.length === 1 ? (
                <circle
                  cx={150}
                  cy={150}
                  r={105}
                  fill="none"
                  strokeWidth={70}
                  style={{ stroke: color(s.i) }}
                />
              ) : (
                <path d={arc(150, 150, 70, 140, a0, angle - 0.012)} style={{ fill: color(s.i) }} />
              )}
            </g>
          );
        })}
        <text x={150} y={146} textAnchor="middle" className="pie-total">
          {fmt(m, total)}
        </text>
        <text x={150} y={168} textAnchor="middle" className="axis">
          total
        </text>
      </svg>
      <ul className="legend">
        {data.map((x, i) => (
          <li key={x.key || "none"}>
            <span className="swatch-sq" style={{ background: color(i) }} aria-hidden="true" />
            <span className="lname">{x.label}</span>
            <span className="lval">{fmt(m, x.value)}</span>
            <span className="lpct dim">
              {total ? `${Math.round((Math.max(x.value, 0) / total) * 100)}%` : ""}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
