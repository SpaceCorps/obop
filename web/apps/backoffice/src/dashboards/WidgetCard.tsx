import { useEffect, useId, useMemo, useState } from "react";
import { useData } from "../data.tsx";
import { integer, label } from "../format.ts";
import { condForKey, encodeState, formatSum } from "../query.ts";
import { Cell } from "../fields.tsx";
import { Button, EmptyState, Icon, IconButton, Notice, Skeleton, Stat } from "../ui.tsx";
import { Cartesian, Pie } from "../views/GraphView.tsx";
import { toFacets } from "./filter.ts";
import {
  buildPoints,
  fetchWidget,
  resolve,
  type Fetched,
  type Widget,
  type WidgetData,
} from "./widget.ts";

/** A widget's data: resolved against what this person may see, then fetched from the server. */
export function useWidgetData(w: Widget) {
  const d = useData();
  const r = useMemo(() => resolve(w, d.models), [w, d.models]);
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
  const id = useId();

  const drillTo = (extra: ReturnType<typeof condForKey>[] = []) => {
    if (!r.ok) return null;
    const facets = [...toFacets(r.conds, r.model), ...extra];
    return d.linkTo(r.model.name, { st: encodeState({ facets, group: [] }) });
  };

  let body;
  if (state === "loading") body = <Skeleton rows={3} cols={2} />;
  else if (state.state === "noaccess")
    body = (
      <div className="wstate" role="status">
        <Icon name="lock" size={22} />
        <strong>No access</strong>
        <span className="hint">You cannot see the data this tile reads.</span>
      </div>
    );
  else if (state.state === "invalid")
    body = (
      <Notice tone="warning" title="This tile is not set up correctly">
        {state.message}
      </Notice>
    );
  else if (state.state === "error")
    body = (
      <div className="wstate">
        <Notice tone="danger" title="Could not load this tile">
          {state.message}
        </Notice>
        <Button size="sm" onClick={retry}>
          Try again
        </Button>
      </div>
    );
  else if (r.ok) body = <Ready w={w} data={state.data} r={r} drillTo={drillTo} id={id} />;

  const kpi = w.kind === "kpi" && state !== "loading" && "data" in (state as object);
  return (
    <article
      className={`wcard kind-${w.kind}${edit ? " editing" : ""}`}
      aria-label={w.title}
      data-kind={w.kind}
    >
      {!kpi && (
        <header>
          <h3>{w.title}</h3>
          {r.ok && drillTo() && !edit && state !== "loading" && "data" in (state as object) && (
            <a className="wopen" href={drillTo()!}>
              Open list
            </a>
          )}
        </header>
      )}
      {edit && (
        <div className="wtools" role="group" aria-label={`Edit ${w.title}`}>
          <IconButton
            icon="arrow-up"
            label={`Move ${w.title} earlier`}
            size="sm"
            disabled={first}
            onClick={() => onMove(-1)}
          />
          <IconButton
            icon="arrow-down"
            label={`Move ${w.title} later`}
            size="sm"
            disabled={last}
            onClick={() => onMove(1)}
          />
          <IconButton icon="edit" label={`Edit ${w.title}`} size="sm" onClick={onEdit} />
          <IconButton icon="trash" label={`Remove ${w.title}`} size="sm" onClick={onRemove} />
        </div>
      )}
      <div className="wbody">{body}</div>
    </article>
  );
}

function Ready({
  w,
  data,
  r,
  drillTo,
  id,
}: {
  w: Widget;
  data: WidgetData;
  r: Extract<ReturnType<typeof resolve>, { ok: true }>;
  drillTo: (extra?: ReturnType<typeof condForKey>[]) => string | null;
  id: string;
}) {
  const d = useData();
  if (data.kind === "kpi") {
    const shown = data.measure ? formatSum(data.measure, data.value) : integer(data.value);
    const to = drillTo();
    const hint = (
      <>
        {data.measure ? `Sum of ${label(data.measure.name).toLowerCase()}` : "Records"}
        {r.conds.length > 0 && " (filtered)"}
        {data.truncated && `, first 5,000 of ${integer(data.total)}`}
      </>
    );
    return to ? (
      <a className="kpi-link" href={to} aria-label={`${w.title}: ${shown}. Open the list.`}>
        <Stat label={w.title} value={shown} hint={hint} />
      </a>
    ) : (
      <Stat label={w.title} value={shown} hint={hint} />
    );
  }

  if (data.kind === "rows") {
    const labelF = r.model.fields.find((f) => f.kind === "text");
    const more = r.model.fields
      .filter(
        (f) => f !== labelF && (f.kind === "money" || f.kind === "select" || f.kind === "date"),
      )
      .slice(0, 2);
    if (data.rows.length === 0) return <p className="hint">No records yet.</p>;
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
            Showing {data.rows.length} of {integer(data.total)}.
          </p>
        )}
      </>
    );
  }

  return <Series w={w} data={data} drillTo={drillTo} id={id} />;
}

function Series({
  w,
  data,
  drillTo,
  id,
}: {
  w: Widget;
  data: Extract<WidgetData, { kind: "series" }>;
  drillTo: (extra?: ReturnType<typeof condForKey>[]) => string | null;
  id: string;
}) {
  const d = useData();
  const { dim, measure } = data;
  const points = useMemo(
    () => buildPoints(data.rows, dim, measure, d.lookup),
    // ref labels arrive after the rows do
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data.rows, dim, measure, d.version],
  );
  const fmt = (n: number) => (measure ? formatSum(measure, n) : integer(n));
  const go = (key: string) => {
    const to = drillTo([condForKey(dim, key)]);
    if (to) location.hash = to;
  };
  const total = points.reduce((n, p) => n + Math.max(p.value, 0), 0);
  const note = data.truncated && `Based on the first 5,000 of ${integer(data.total)} records.`;

  if (points.length === 0 || data.total === 0) return <p className="hint">No data yet.</p>;

  if (w.kind === "table")
    return (
      <>
        <table className="table dense wtable">
          <caption className="sr-only">{w.title}</caption>
          <thead>
            <tr>
              <th scope="col">{label(dim.name)}</th>
              <th scope="col" className="num">
                {measure ? label(measure.name) : "Records"}
              </th>
            </tr>
          </thead>
          <tbody>
            {points.map((p) => {
              const to = drillTo([condForKey(dim, p.key)]);
              return (
                <tr key={p.key || "none"}>
                  <th scope="row">{to ? <a href={to}>{p.label}</a> : p.label}</th>
                  <td className="num tabular">{fmt(p.value)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {note && <p className="wnote">{note}</p>}
      </>
    );

  const summary = `${w.title}. ${points.map((p) => `${p.label} ${fmt(p.value)}`).join(", ")}.`;
  return (
    <figure className="chart wchart">
      {w.kind === "pie" ? (
        <Pie data={points} m={measure} total={total} id={id} summary={summary} onPick={go} />
      ) : (
        <Cartesian
          data={points}
          m={measure}
          type={w.kind === "line" ? "line" : "bar"}
          id={id}
          summary={summary}
          onPick={go}
        />
      )}
      {note && <figcaption className="wnote">{note}</figcaption>}
    </figure>
  );
}

export function NoDashboards(p: { onCreate?: () => void; busy: boolean }) {
  return (
    <EmptyState
      icon="graph"
      title="No dashboards yet"
      hint={
        p.onCreate
          ? "Start with a dashboard built from the apps you have installed: record counts and money totals you can change later."
          : "Ask someone who manages dashboards to create one."
      }
      action={
        p.onCreate
          ? { label: p.busy ? "Creating…" : "Create a starter dashboard", onClick: p.onCreate }
          : undefined
      }
    />
  );
}
