/**
 * A key figure: a label, one big number and a caption in words, optionally a link to the records
 * behind it. The dashboards draw their number tiles with it and the app overviews can too; `useKpi`
 * reads the number from the server (D4), one value per currency.
 */
import type { ReactNode } from "react";
import { t } from "@obop/ui";
import type { ModelMeta, ServerFilter, ValueTone } from "../api.ts";
import {
  cellValue,
  currenciesOf,
  formatMeasure,
  type MeasureSel,
  type ReportRow,
} from "../reports.ts";
import { useReportRows } from "../useAggregate.ts";

export type KpiTileProps = {
  label: string;
  /** The figure, formatted ("€25,338.80", "8"). */
  value?: ReactNode;
  /** The same figure in other currencies, each formatted ("SEK 1,200.00"). */
  more?: string[];
  /** A line in words under the figure: what it counts ("Sale orders that are confirmed"). */
  caption?: ReactNode;
  /** Where the records behind the figure are listed; the whole tile is then a link. */
  href?: string;
  /** The link's accessible name; "{label}: {value}. Open the list." when absent. */
  linkLabel?: string;
  /** Shows a placeholder while the figure loads. */
  loading?: boolean;
  /** Colours the figure by meaning (a number that needs attention is `warn` or `bad`). */
  tone?: ValueTone;
  /** No border or padding of its own, for a tile drawn inside another card. */
  plain?: boolean;
};

/** One key figure. Put several in a `KpiStrip`. */
export function KpiTile(p: KpiTileProps) {
  const cls = `kpi${p.plain ? " plain" : ""}${p.tone ? ` tone-${p.tone}` : ""}${p.href ? " kpi-link" : ""}`;
  const body = (
    <>
      <span className="kpi-label">{p.label}</span>
      {p.loading ? (
        <span className="kpi-wait" aria-hidden="true">
          <span className="skel" />
          <span className="skel short" />
        </span>
      ) : (
        <>
          <strong className="kpi-value stat-value tabular">{p.value}</strong>
          {p.more && p.more.length > 0 && (
            <span className="kpi-more tabular">{p.more.join(" · ")}</span>
          )}
        </>
      )}
      {p.caption && <span className="kpi-caption">{p.caption}</span>}
    </>
  );
  if (p.href && !p.loading)
    return (
      <a
        className={cls}
        href={p.href}
        aria-label={
          p.linkLabel ??
          t("{title}: {value}. Open the list.", {
            title: p.label,
            value: [typeof p.value === "string" ? p.value : "", ...(p.more ?? [])]
              .filter(Boolean)
              .join(", "),
          })
        }
      >
        {body}
      </a>
    );
  return (
    <div className={cls} aria-busy={p.loading || undefined}>
      {body}
    </div>
  );
}

/** A row of key figures that wraps on narrow screens. */
export function KpiStrip({ children, label }: { children: ReactNode; label?: string }) {
  return (
    <div className="kpistrip" role={label ? "group" : undefined} aria-label={label}>
      {children}
    </div>
  );
}

/**
 * The figure of a report without dimensions, formatted: in the first currency (the instance's when
 * it has records) and in each other currency. A count or a non-money sum has one value.
 */
export function kpiValues(
  rows: readonly ReportRow[],
  measure: MeasureSel,
): { value: string; more: string[] } {
  if (rows.length === 0) return { value: formatMeasure(measure, 0), more: [] };
  const text = (cur: string | undefined) => {
    const mine = rows.filter((x) => x.currency === cur);
    const cell = {
      count: mine.reduce((n, x) => n + x.count, 0),
      value: mine.reduce((n, x) => n + x.value, 0),
    };
    return formatMeasure(measure, cellValue(measure, cell), cur);
  };
  const [first, ...rest] = currenciesOf(rows);
  return { value: text(first), more: rest.map(text) };
}

/** A key figure from the server: the count or the sum of a measure over the matching records. */
export function useKpi(q: {
  model: ModelMeta;
  filter?: ServerFilter[];
  measure: MeasureSel;
  enabled?: boolean;
}): {
  /** The figure in the first currency (the instance's when it has records), formatted. */
  value: string;
  /** The figure in each other currency, formatted. */
  more: string[];
  /** Records the filter matches. */
  count: number;
  loaded: boolean;
  error?: string;
  retry: () => void;
} {
  const r = useReportRows({
    server: true,
    enabled: q.enabled,
    model: q.model,
    filter: q.filter ?? [],
    dims: [],
    measure: q.measure,
  });
  return {
    ...kpiValues(r.rows, q.measure),
    count: r.total,
    loaded: r.loaded,
    error: r.error,
    retry: r.retry,
  };
}
