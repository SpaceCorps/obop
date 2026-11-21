import { useEffect, useMemo, useState } from "react";
import { fmt, inSentence, t } from "@obop/ui";
import type { ModelMeta, Row } from "../api.ts";
import { Chart, OTHER_KEY } from "../charts/Chart.tsx";
import { KpiTile, kpiValues } from "../charts/KpiTile.tsx";
import { useData } from "../data.tsx";
import { integer } from "../format.ts";
import { fieldTitle, modelTitle } from "../labels.ts";
import { currencyOf as presentCurrency } from "../presentation.ts";
import { encodeState, type Cond } from "../query.ts";
import {
  compactMeasure,
  currenciesOf,
  dimCond,
  dimTitle,
  formatMeasure,
  isDated,
  reportFromGroups,
  type ReportRow,
} from "../reports.ts";
import { serverMessage } from "../serverMessage.ts";
import { Cell } from "../fields.tsx";
import { Button, EmptyState, Icon, IconButton, Notice, Skeleton } from "../ui.tsx";
import { usePresentCtx, useTitleLookup } from "../useAggregate.ts";
import { chartPoints, otherCond, tonedDim } from "../views/GraphView.tsx";
import { conditionsText, toFacets } from "./filter.ts";
import { fetchWidget, resolve, type Fetched, type Widget, type WidgetData } from "./widget.ts";

/** A widget's data: resolved against what this person may see, then fetched from the server. */
export function useWidgetData(w: Widget) {
  const d = useData();
  // `resolve` writes the detail sentence of an invalid tile ("A sum needs a measure field", the
  // filter's own errors), so it runs again in the new language. The titles "This tile is not set up
  // correctly" and "No access" are written where the card draws, not here.
  const r = useMemo(() => resolve(w, d.models), [w, d.models, d.lang]);
  const [got, setGot] = useState<Fetched | null>(null);
  const [attempt, setAttempt] = useState(0);
  const key = JSON.stringify([
    w.kind,
    w.model_name,
    w.measure_field,
    w.aggregate,
    w.group_by,
    w.filter,
  ]);
  useEffect(() => {
    if (!r.ok) return;
    let live = true;
    void fetchWidget(d.token, w, r).then((f) => live && setGot(f));
    return () => {
      live = false;
    };
    // `key` stands for the widget's definition; `mutations` refreshes numbers after any write.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, r.ok, d.token, d.mutations, attempt]);
  const state: Fetched | "loading" | { state: "invalid"; message: string } = !r.ok
    ? r.reason === "noaccess"
      ? { state: "noaccess", detail: r.detail }
      : { state: "invalid", message: r.detail }
    : (got ?? "loading");
  return { r, state, retry: () => setAttempt((n) => n + 1) };
}

type Resolved = Extract<ReturnType<typeof resolve>, { ok: true }>;
type ReportData = Extract<WidgetData, { kind: "report" }>;

type Props = {
  widget: Widget;
  edit: boolean;
  first: boolean;
  last: boolean;
  onEdit: () => void;
  onRemove: () => void;
  onMove: (delta: -1 | 1) => void;
};

export function WidgetCard({ widget: w, edit, first, last, onEdit, onRemove, onMove }: Props) {
  const d = useData();
  const { r, state, retry } = useWidgetData(w);

  const drillTo = (extra: Cond[] = []) => {
    if (!r.ok) return null;
    const facets = [...toFacets(r.conds, r.model), ...extra];
    return d.linkTo(r.model.name, { st: encodeState({ facets, group: [] }) });
  };

  const ready = state !== "loading" && state.state === "ready";
  let body;
  if (state === "loading")
    body =
      w.kind === "kpi" ? <KpiTile plain label={w.title} loading /> : <Skeleton rows={3} cols={2} />;
  else if (state.state === "noaccess")
    body = (
      <div className="wstate" role="status">
        <Icon name="lock" size={22} />
        <strong>{t("No access")}</strong>
        <span className="hint">{t("You cannot see the data this tile reads.")}</span>
      </div>
    );
  else if (state.state === "invalid")
    body = (
      <Notice tone="warning" title={t("This tile is not set up correctly")}>
        {state.message}
      </Notice>
    );
  else if (state.state === "error")
    body = (
      <div className="wstate">
        <Notice tone="danger" title={t("Could not load this tile")}>
          {serverMessage(state.message)}
        </Notice>
        <Button size="sm" onClick={retry}>
          {t("Try again")}
        </Button>
      </div>
    );
  else if (r.ok) body = <Ready w={w} data={state.data} r={r} drillTo={drillTo} />;

  // A number tile names itself; the others have a heading with a link to their list.
  const kpi = w.kind === "kpi" && (ready || state === "loading");
  return (
    <article
      className={`wcard kind-${w.kind}${edit ? " editing" : ""}`}
      aria-label={w.title}
      data-kind={w.kind}
    >
      {!kpi && (
        <header>
          <h3>{w.title}</h3>
          {r.ok && ready && !edit && drillTo() && (
            <a className="wopen" href={drillTo()!}>
              {t("Open list")}
            </a>
          )}
        </header>
      )}
      <div className="wbody">{body}</div>
      {edit && (
        <div className="wtools" role="group" aria-label={t("Edit {title}", { title: w.title })}>
          <IconButton
            icon="arrow-up"
            label={t("Move {title} earlier", { title: w.title })}
            size="sm"
            disabled={first}
            onClick={() => onMove(-1)}
          />
          <IconButton
            icon="arrow-down"
            label={t("Move {title} later", { title: w.title })}
            size="sm"
            disabled={last}
            onClick={() => onMove(1)}
          />
          <IconButton
            icon="edit"
            label={t("Edit {title}", { title: w.title })}
            size="sm"
            onClick={onEdit}
          />
          <IconButton
            icon="trash"
            label={t("Remove {title}", { title: w.title })}
            size="sm"
            onClick={onRemove}
          />
        </div>
      )}
    </article>
  );
}

/** What a number tile counts, in words: "Deal records where Stage is Won". */
export function kpiCaption(
  r: Pick<Resolved, "model" | "conds" | "measure">,
  moduleId: string,
  refText: (model: string, id: string) => string,
): string {
  const model = modelTitle(r.model, moduleId);
  const conds = r.conds.length ? conditionsText(r.conds, r.model, refText) : "";
  if (r.measure) {
    const field = inSentence(fieldTitle(r.measure));
    return conds
      ? t("Sum of {field} where {conditions}", { field, conditions: conds })
      : t("Sum of {field} over all {model} records", { field, model: inSentence(model) });
  }
  return conds
    ? t("{model} records where {conditions}", { model, conditions: conds })
    : t("All {model} records", { model: inSentence(model) });
}

/** The rows of a widget's report, one per currency for money. */
function useReportOf(data: ReportData, model: ModelMeta): ReportRow[] {
  const d = useData();
  const ctx = usePresentCtx();
  return useMemo(() => {
    const field = data.measure?.field;
    const toCurrency = (v: unknown) =>
      data.split
        ? presentCurrency({ id: "", [data.split]: v } as Row, model, ctx, field)
        : ctx.instanceCurrency;
    return reportFromGroups(
      data.answer.groups,
      data.dim ? [data.dim] : [],
      data.measure,
      toCurrency,
    );
    // `version` brings the rows a `currency_from` path reads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, model, d.version]);
}

function Ready({
  w,
  data,
  r,
  drillTo,
}: {
  w: Widget;
  data: WidgetData;
  r: Resolved;
  drillTo: (extra?: Cond[]) => string | null;
}) {
  const d = useData();
  if (data.kind === "rows") {
    const labelF = r.model.fields.find((f) => f.kind === "text");
    const more = r.model.fields
      .filter(
        (f) => f !== labelF && (f.kind === "money" || f.kind === "select" || f.kind === "date"),
      )
      .slice(0, 2);
    if (data.rows.length === 0) return <p className="hint">{t("No records yet.")}</p>;
    return (
      <>
        <table className="table dense wtable">
          <caption className="sr-only">{w.title}</caption>
          <tbody>
            {data.rows.map((row) => {
              const to = d.linkTo(r.model.name, { id: row.id });
              const text = d.recordLabel(r.model.name, row);
              return (
                <tr key={row.id}>
                  <th scope="row">{to ? <a href={to}>{text}</a> : text}</th>
                  {more.map((f) => (
                    <td key={f.name}>
                      <Cell field={f} value={row[f.name]} link={false} />
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
        {data.total > data.rows.length && (
          <p className="wnote">
            {t("Showing {shown} of {total}.", {
              shown: data.rows.length,
              total: integer(data.total),
            })}
          </p>
        )}
      </>
    );
  }
  return w.kind === "kpi" ? (
    <Kpi w={w} data={data} r={r} drillTo={drillTo} />
  ) : (
    <Series w={w} data={data} r={r} drillTo={drillTo} />
  );
}

function Kpi({
  w,
  data,
  r,
  drillTo,
}: {
  w: Widget;
  data: ReportData;
  r: Resolved;
  drillTo: (extra?: Cond[]) => string | null;
}) {
  const d = useData();
  const rows = useReportOf(data, r.model);
  const { value, more } = kpiValues(rows, data.measure);
  const moduleId = d.models.get(r.model.name)?.module.id ?? "";
  return (
    <KpiTile
      plain
      label={w.title}
      value={value}
      more={more}
      caption={kpiCaption(r, moduleId, d.refText)}
      href={drillTo() ?? undefined}
    />
  );
}

function Series({
  w,
  data,
  r,
  drillTo,
}: {
  w: Widget;
  data: ReportData;
  r: Resolved;
  drillTo: (extra?: Cond[]) => string | null;
}) {
  const d = useData();
  const rows = useReportOf(data, r.model);
  const dim = data.dim!;
  const m = data.measure;
  const curs = currenciesOf(rows);
  const cur = curs[0];
  const lk = useTitleLookup();
  const points = useMemo(
    () =>
      chartPoints(
        r.model,
        dim,
        m,
        rows.filter((x) => x.currency === cur),
        lk,
      ),
    // ref labels arrive after the groups do
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, cur, lk, d.lang, d.locale],
  );
  const format = (n: number) => formatMeasure(m, n, cur);
  const keep = w.kind === "pie" ? 7 : 8;
  const go = (key: string) => {
    const c = key === OTHER_KEY ? otherCond(dim, points, keep) : dimCond(dim, key);
    const to = c && drillTo([c]);
    if (to) location.hash = to;
  };
  const others = curs.slice(1).filter((c): c is string => !!c);
  const note =
    others.length > 0 && cur
      ? t("Amounts in {currency}; records in {others} are not included.", {
          currency: cur,
          others: fmt.list(others, "conjunction"),
        })
      : null;

  if (rows.length === 0 || points.length === 0) return <p className="hint">{t("No data yet.")}</p>;

  if (w.kind === "table")
    return (
      <>
        <table className="table dense wtable">
          <caption className="sr-only">{w.title}</caption>
          <thead>
            <tr>
              <th scope="col">{dimTitle(dim)}</th>
              <th scope="col" className="num">
                {m ? fieldTitle(m.field) : t("Records")}
              </th>
            </tr>
          </thead>
          <tbody>
            {points.map((p) => {
              const to = drillTo([dimCond(dim, p.key)]);
              const label = p.full ?? p.label;
              return (
                <tr key={p.key || "none"}>
                  <th scope="row">{to ? <a href={to}>{label}</a> : label}</th>
                  <td className="num tabular">{format(p.value)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {note && <p className="wnote">{note}</p>}
      </>
    );

  const summary = t("{title}. {values}.", {
    title: w.title,
    values: points.map((p) => `${p.full ?? p.label} ${format(p.value)}`).join(", "),
  });
  const kind = w.kind === "pie" ? "pie" : w.kind === "line" ? "line" : "bar";
  const toned = tonedDim(r.model, dim.field);
  const distinct = new Set(points.map((p) => p.tone)).size === points.length;
  const colors =
    toned && (kind !== "pie" || distinct) ? "tone" : kind === "pie" ? "palette" : "single";
  return (
    <figure className="chart wchart">
      <Chart
        kind={kind}
        data={points}
        format={format}
        compact={(n) => compactMeasure(m, n, cur)}
        title={w.title}
        summary={summary}
        onPick={go}
        ordered={isDated(dim.field)}
        colors={colors}
        maxHeight={230}
      />
      {note && <figcaption className="wnote">{note}</figcaption>}
    </figure>
  );
}

export function NoDashboards(p: { onCreate?: () => void; busy: boolean }) {
  return (
    <EmptyState
      icon="graph"
      title={t("No dashboards yet")}
      hint={
        p.onCreate
          ? t(
              "Start with a dashboard built from the apps you have installed: record counts and money totals you can change later.",
            )
          : t("Ask someone who manages dashboards to create one.")
      }
      action={
        p.onCreate
          ? {
              label: p.busy ? t("Creating…") : t("Create a starter dashboard"),
              onClick: p.onCreate,
            }
          : undefined
      }
    />
  );
}
