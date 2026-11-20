/**
 * One chart for the graph view, the dashboard tiles and the app overviews (UI plan 2.5.3). It is
 * drawn at the real pixel size of its container (a ResizeObserver), so its text is 12 px on every
 * screen instead of a fixed drawing scaled down to 7 px. A categorical bar chart with more than 10
 * groups, or one whose labels would collide, turns into horizontal bars sorted by value, showing
 * the top 8 and "Other". A single series has one accent colour; a status uses its badge tones.
 */
import { useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { fmt, t } from "@obop/ui";

/** One group of a chart. */
export type ChartDatum = {
  key: string;
  /** The short label on the axis or in the legend. */
  label: string;
  value: number;
  /** The long label for a tooltip and a screen reader ("Week of 9 Nov 2026"); `label` if absent. */
  full?: string;
  /** A tone class (`t-ok`, `t-bad`, `t-1`…) whose `--tone` colours the mark, for `colors: "tone"`. */
  tone?: string;
};

export type ChartKind = "bar" | "line" | "pie";

export type ChartProps = {
  kind: ChartKind;
  data: ChartDatum[];
  /** The full text of a value ("€1,250.00"). */
  format: (n: number) => string;
  /** A short text for axis ticks and labels on marks ("€1.3K"); `format` when absent. */
  compact?: (n: number) => string;
  /** The chart's short name ("Bar chart"); `summary` is the long description. */
  title: string;
  summary: string;
  /** Called with a datum's key (or `OTHER_KEY`) when a mark is chosen; marks are buttons only then. */
  onPick?: (key: string) => void;
  /** A series in time: keep the order, never fold into "Other", thin the labels instead. */
  ordered?: boolean;
  /** One accent colour, each datum's tone, or the category palette (a pie's default). */
  colors?: "single" | "tone" | "palette";
  /** The tallest the drawing gets, in px; its width is the container's. */
  maxHeight?: number;
  /**
   * How the value of the folded "Other" group is made from the values of the rest: their sum when
   * absent, the smallest or the largest of them for a smallest or a largest value.
   */
  merge?: (values: number[]) => number;
  className?: string;
};

/** The key of the folded "Other" group. */
export const OTHER_KEY = "\u0000other";
/** More groups than this turn a categorical bar chart horizontal and fold the rest into Other. */
export const MAX_BARS = 10;
const TOP_BARS = 8;
const TOP_SLICES = 7;
const FONT = 12;

/** The width of an element, followed as it changes (0 until it is known). */
export function useWidth<T extends HTMLElement>(): [React.RefObject<T | null>, number] {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let frame = 0;
    const read = () => {
      const w = Math.floor(el.getBoundingClientRect().width);
      setWidth((old) => (Math.abs(old - w) >= 1 ? w : old));
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    // A frame later, so a scrollbar that the new height brings cannot loop the observer.
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(read);
    });
    ro.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, []);
  return [ref, width];
}

let canvas: CanvasRenderingContext2D | null | undefined;
/** The width of a text at the chart's font size, measured once per text. */
function measurer(font: string) {
  const cache = new Map<string, number>();
  return (s: string): number => {
    let w = cache.get(s);
    if (w !== undefined) return w;
    if (canvas === undefined)
      try {
        canvas = document.createElement("canvas").getContext("2d");
      } catch {
        canvas = null;
      }
    if (canvas) {
      canvas.font = font;
      w = canvas.measureText(s).width;
    } else w = s.length * FONT * 0.56;
    cache.set(s, w);
    return w;
  };
}

/** Shorten a text with an ellipsis until it fits `max` px. */
function fit(s: string, max: number, width: (s: string) => number): string {
  if (width(s) <= max) return s;
  let lo = 0;
  let hi = s.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (width(`${s.slice(0, mid)}…`) <= max) lo = mid;
    else hi = mid - 1;
  }
  return lo > 0 ? `${s.slice(0, lo).trimEnd()}…` : "";
}

/** Round axis bounds and ticks for a range that always includes zero. */
export function niceTicks(
  lo: number,
  hi: number,
  count = 4,
): { lo: number; hi: number; ticks: number[] } {
  const min = Math.min(0, lo);
  const max = Math.max(0, hi);
  if (max - min <= 0) return { lo: 0, hi: 1, ticks: [0, 1] };
  const raw = (max - min) / count;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((k) => k * pow).find((u) => u >= raw) ?? raw;
  const top = Math.ceil(max / step) * step;
  const bottom = Math.floor(min / step) * step;
  const ticks: number[] = [];
  for (let v = bottom; v <= top + step / 2; v += step) ticks.push(Math.round(v / step) * step);
  return { lo: bottom, hi: top, ticks };
}

const sum = (values: number[]) => values.reduce((n, v) => n + v, 0);

/** The top groups by value and the rest added up (or `merge`d) as "Other". */
export function foldOther(
  data: ChartDatum[],
  keep: number,
  merge: (values: number[]) => number = sum,
): ChartDatum[] {
  if (data.length <= keep + 1) return data;
  const sorted = [...data].sort((a, b) => b.value - a.value);
  const rest = sorted.slice(keep);
  const top = sorted.slice(0, keep);
  const other = merge(rest.map((x) => x.value));
  // "None" is shown after the named groups, whatever its size.
  return [
    ...top.filter((x) => x.key !== ""),
    ...top.filter((x) => x.key === ""),
    {
      key: OTHER_KEY,
      label: t("Other"),
      full: t("Other ({count})", { count: rest.length }),
      value: other,
    },
  ];
}

const activate = (fn: () => void) => ({
  role: "button" as const,
  tabIndex: 0,
  onClick: fn,
  onKeyDown: (e: KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      fn();
    }
  },
});

function fillOf(d: ChartDatum, i: number, colors: ChartProps["colors"]): string {
  if (d.key === OTHER_KEY) return "var(--chart-other)";
  if (colors === "tone" && d.tone) return "var(--tone)";
  if (colors === "palette") return `var(--chart-${(i % 8) + 1})`;
  return "var(--chart-1)";
}

/** A chart that fills the width of its container. */
export function Chart(p: ChartProps) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [font, setFont] = useState(`${FONT}px sans-serif`);
  useLayoutEffect(() => {
    if (ref.current) setFont(`${FONT}px ${getComputedStyle(ref.current).fontFamily}`);
  }, [ref]);
  const measure = useRef<{ font: string; fn: (s: string) => number } | null>(null);
  if (!measure.current || measure.current.font !== font)
    measure.current = { font, fn: measurer(font) };
  const textW = measure.current.fn;
  const compact = p.compact ?? p.format;
  return (
    <div ref={ref} className={`chartbox${p.className ? ` ${p.className}` : ""}`}>
      {width > 0 &&
        (p.kind === "pie" ? (
          <Donut {...p} width={width} textW={textW} />
        ) : (
          <Cartesian {...p} compact={compact} width={width} textW={textW} />
        ))}
    </div>
  );
}

type Inner = ChartProps & { width: number; textW: (s: string) => number };

function markLabel(p: ChartProps, d: ChartDatum): string {
  return p.onPick
    ? t("{label}: {value}. Show records.", { label: d.full ?? d.label, value: p.format(d.value) })
    : t("{label}: {value}", { label: d.full ?? d.label, value: p.format(d.value) });
}

function Cartesian(p: Inner & { compact: (n: number) => string }) {
  const { width: W, textW } = p;
  const categorical = !p.ordered;
  const crowded = p.data.length > 0 && W / p.data.length < 56;
  const horizontal =
    p.kind === "bar" && categorical && (p.data.length > MAX_BARS || (crowded && p.data.length > 1));
  if (horizontal) return <Horizontal {...p} />;

  const data = p.data;
  const n = Math.max(data.length, 1);
  const H = Math.round(Math.min(p.maxHeight ?? 320, Math.max(200, W * 0.62)));
  const values = data.map((d) => d.value);
  const scale = niceTicks(Math.min(0, ...values), Math.max(0, ...values), H < 240 ? 3 : 4);
  const tickW = Math.max(...scale.ticks.map((v) => textW(p.compact(v))));
  const M = { l: Math.ceil(tickW) + 12, r: 8, t: 22, b: 28 };
  const iw = Math.max(W - M.l - M.r, 10);
  const ih = H - M.t - M.b;
  const band = iw / n;
  const y = (v: number) => M.t + ((scale.hi - v) / (scale.hi - scale.lo)) * ih;
  const cx = (i: number) => M.l + band * i + band / 2;
  const bw = Math.max(Math.min(band * 0.62, 64), 2);
  // Labels: each fits its band; in a time series, every k-th label so none overlap.
  const labelW = Math.max(...data.map((d) => textW(d.label)), 0);
  const step = p.ordered ? Math.max(1, Math.ceil((Math.min(labelW, 96) + 10) / band)) : 1;
  const cut = (s: string) => fit(s, step > 1 ? band * step - 10 : band - 6, textW);
  const showValues = data.length <= 16 && data.every((d) => textW(p.compact(d.value)) <= band - 2);
  const pts = data.map((d, i) => `${cx(i)},${y(d.value)}`).join(" ");
  const y0 = y(0);
  return (
    <svg
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      className="chartsvg"
      role="group"
      aria-label={p.title}
    >
      <desc>{p.summary}</desc>
      {scale.ticks.map((v) => (
        <g key={v}>
          <line
            x1={M.l}
            x2={W - M.r}
            y1={y(v)}
            y2={y(v)}
            className={v === 0 ? "grid zero" : "grid"}
          />
          <text x={M.l - 8} y={y(v)} className="axis" textAnchor="end" dominantBaseline="middle">
            {p.compact(v)}
          </text>
        </g>
      ))}
      {p.kind === "line" && data.length > 0 && (
        <>
          <polygon points={`${cx(0)},${y0} ${pts} ${cx(data.length - 1)},${y0}`} className="area" />
          <polyline points={pts} className="line" />
        </>
      )}
      {data.map((d, i) => {
        const label = i % step === 0 ? cut(d.label) : "";
        const top = y(Math.max(d.value, 0));
        const bottom = y(Math.min(d.value, 0));
        const pick = p.onPick;
        return (
          <g key={d.key || "none"}>
            <g
              {...(pick ? activate(() => pick(d.key)) : { role: "img" })}
              aria-label={markLabel(p, d)}
              className={`mark${d.tone && p.colors === "tone" ? ` ${d.tone}` : ""}`}
            >
              <title>{`${d.full ?? d.label}: ${p.format(d.value)}`}</title>
              {p.kind === "bar" ? (
                <rect
                  x={cx(i) - bw / 2}
                  width={bw}
                  y={top}
                  height={Math.max(bottom - top, d.value === 0 ? 0 : 1)}
                  rx={Math.min(4, bw / 4)}
                  style={{ fill: fillOf(d, i, p.colors) }}
                />
              ) : (
                <>
                  {/* The whole band is the target, so a 5 px dot is easy to tap. */}
                  <rect x={cx(i) - band / 2} width={band} y={M.t} height={ih} className="hit" />
                  <circle cx={cx(i)} cy={y(d.value)} r={4.5} className="dot" />
                </>
              )}
            </g>
            {showValues && (
              <text
                x={cx(i)}
                y={d.value < 0 ? bottom + 14 : top - 7}
                textAnchor="middle"
                className="vlabel-t"
              >
                {p.compact(d.value)}
              </text>
            )}
            {label && (
              <text x={cx(i)} y={H - M.b + 18} className="axis" textAnchor="middle">
                {label !== d.label && <title>{d.full ?? d.label}</title>}
                {label}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

function Horizontal(p: Inner & { compact: (n: number) => string }) {
  const { width: W, textW } = p;
  const data = p.data.length > MAX_BARS ? foldOther(p.data, TOP_BARS, p.merge) : [...p.data];
  if (p.data.length <= MAX_BARS)
    data.sort((a, b) => Number(a.key === "") - Number(b.key === "") || b.value - a.value);
  const rowH = 30;
  const barH = 16;
  const H = data.length * rowH + 6;
  const values = data.map((d) => d.value);
  const lo = Math.min(0, ...values);
  const hi = Math.max(0, ...values);
  const labelW = Math.min(
    Math.max(...data.map((d) => textW(d.label))) + 12,
    Math.max(W * 0.4, Math.min(W * 0.5, 96)),
  );
  const valueW = Math.max(...data.map((d) => textW(p.compact(d.value)))) + 12;
  const plot = Math.max(W - labelW - valueW, 24);
  const span = hi - lo || 1;
  const x = (v: number) => labelW + ((v - lo) / span) * plot;
  const x0 = x(0);
  return (
    <svg
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      className="chartsvg horizontal"
      role="group"
      aria-label={p.title}
    >
      <desc>{p.summary}</desc>
      <line x1={x0} x2={x0} y1={0} y2={H} className="grid zero" />
      {data.map((d, i) => {
        const yc = i * rowH + rowH / 2 + 3;
        const a = x(Math.min(d.value, 0));
        const b = x(Math.max(d.value, 0));
        const label = fit(d.label, labelW - 12, textW);
        const pick = p.onPick;
        return (
          <g key={d.key || "none"}>
            <text
              x={labelW - 10}
              y={yc}
              className="axis"
              textAnchor="end"
              dominantBaseline="middle"
            >
              {label !== d.label && <title>{d.full ?? d.label}</title>}
              {label}
            </text>
            <g
              {...(pick ? activate(() => pick(d.key)) : { role: "img" })}
              aria-label={markLabel(p, d)}
              className={`mark${d.tone && p.colors === "tone" ? ` ${d.tone}` : ""}`}
            >
              <title>{`${d.full ?? d.label}: ${p.format(d.value)}`}</title>
              <rect
                x={labelW - 4}
                width={W - labelW + 4}
                y={yc - rowH / 2}
                height={rowH}
                className="hit"
              />
              <rect
                x={a}
                width={Math.max(b - a, d.value === 0 ? 0 : 1)}
                y={yc - barH / 2}
                height={barH}
                rx={4}
                style={{ fill: fillOf(d, i, p.colors) }}
              />
            </g>
            <text
              x={d.value < 0 ? a - 6 : b + 6}
              y={yc}
              className="vlabel-t"
              textAnchor={d.value < 0 ? "end" : "start"}
              dominantBaseline="middle"
            >
              {p.compact(d.value)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

function arc(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  const pt = (r: number, a: number) => `${cx + r * Math.cos(a)} ${cy + r * Math.sin(a)}`;
  const large = a1 - a0 > Math.PI ? 1 : 0;
  return `M ${pt(r1, a0)} A ${r1} ${r1} 0 ${large} 1 ${pt(r1, a1)} L ${pt(r0, a1)} A ${r0} ${r0} 0 ${large} 0 ${pt(r0, a0)} Z`;
}

function Donut(p: Inner) {
  const all = p.data.length > TOP_SLICES + 1 ? foldOther(p.data, TOP_SLICES, p.merge) : p.data;
  const total = all.reduce((n, d) => n + Math.max(d.value, 0), 0);
  const size = Math.round(Math.max(150, Math.min(p.width, 240)));
  const c = size / 2;
  const r1 = c - 4;
  const r0 = r1 * 0.58;
  const slices = all.map((d, i) => ({ d, i })).filter((s) => s.d.value > 0);
  let angle = -Math.PI / 2;
  const shareText = (v: number) => (total ? fmt.percent(Math.max(v, 0) / total, 0) : "");
  return (
    <div className="piewrap">
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="piesvg"
        role="group"
        aria-label={p.title}
      >
        <desc>{p.summary}</desc>
        {slices.map(({ d, i }) => {
          const share = d.value / total;
          const a0 = angle;
          angle += share * Math.PI * 2;
          const text = t("{label}: {value}, {share}. Show records.", {
            label: d.full ?? d.label,
            value: p.format(d.value),
            share: fmt.percent(share, 0),
          });
          const pick = p.onPick;
          return (
            <g
              key={d.key || "none"}
              {...(pick ? activate(() => pick(d.key)) : { role: "img" })}
              aria-label={pick ? text : text.replace(t(" Show records."), "")}
              className={`mark${d.tone && p.colors === "tone" ? ` ${d.tone}` : ""}`}
            >
              <title>{text.replace(t(" Show records."), "")}</title>
              {slices.length === 1 ? (
                <circle
                  cx={c}
                  cy={c}
                  r={(r0 + r1) / 2}
                  fill="none"
                  strokeWidth={r1 - r0}
                  style={{ stroke: fillOf(d, i, p.colors ?? "palette") }}
                />
              ) : (
                <path
                  d={arc(c, c, r0, r1, a0, angle - Math.min(0.012, share))}
                  style={{ fill: fillOf(d, i, p.colors ?? "palette") }}
                />
              )}
            </g>
          );
        })}
        <text x={c} y={c - 2} textAnchor="middle" className="pie-total">
          {(p.compact ?? p.format)(total)}
        </text>
        <text x={c} y={c + 16} textAnchor="middle" className="axis">
          {t("total")}
        </text>
      </svg>
      <ul className="legend">
        {all.map((d, i) => (
          <li key={d.key || "none"} className={d.tone && p.colors === "tone" ? d.tone : undefined}>
            <span
              className="swatch-sq"
              style={{ background: fillOf(d, i, p.colors ?? "palette") }}
              aria-hidden="true"
            />
            <span className="lname">{d.full ?? d.label}</span>
            <span className="lval">{p.format(d.value)}</span>
            <span className="lpct dim">{shareText(d.value)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
