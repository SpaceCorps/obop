import { useCallback, useEffect, useMemo, useState, type DragEvent } from "react";
import { call, type FieldMeta, type ModelMeta, type Row } from "./api.ts";
import type { App } from "./Chrome.tsx";
import { RecordPanel } from "./RecordPanel.tsx";
import {
  display,
  isMasked,
  isMoney,
  isState,
  label,
  modelLabel,
  money,
  visibleFields,
} from "./format.ts";
import type { Route } from "./route.ts";

type Props = {
  token: string;
  app: App;
  model: ModelMeta;
  route: Route;
  go: (r: Partial<Route>) => void;
  notify: (text: string) => void;
};

function storedView(model: string): string {
  try {
    return localStorage.getItem(`obop-view-${model}`) ?? "";
  } catch {
    return "";
  }
}

export function ModelPage({ token, app, model, route, go, notify }: Props) {
  const [rows, setRows] = useState<Row[]>([]);
  const [withheld, setWithheld] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const fields = useMemo(() => visibleFields(model.fields), [model]);
  const stateField = fields.find((f) => isState(f.name))?.name;
  const [groupBy, setGroupBy] = useState<string>(stateField ?? "");
  const view = route.view || storedView(model.name) || (stateField ? "kanban" : "list");
  const title = modelLabel(model.name, app.module.id);

  const load = useCallback(async () => {
    const r = await call(token, `${model.name}.list`);
    if (r.kind === "ok") {
      setRows(r.value as Row[]);
      setWithheld(r.denied);
    }
    setLoading(false);
  }, [token, model.name]);

  useEffect(() => {
    setLoading(true);
    setQuery("");
    setGroupBy(stateField ?? "");
    void load();
  }, [load, stateField]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((r) =>
      fields.some((f) => display(f.name, r[f.name]).toLowerCase().includes(q)),
    );
  }, [rows, query, fields]);

  const setView = (v: string) => {
    try {
      localStorage.setItem(`obop-view-${model.name}`, v);
    } catch {
      /* ignore */
    }
    go({ app: route.app, model: route.model, view: v, id: route.id });
  };

  const open = (id: string) => go({ app: route.app, model: route.model, view, id });
  const selected = route.id === "new" ? "new" : rows.find((r) => r.id === route.id);

  return (
    <>
      <section className="main panel">
        <div className="controlpanel">
          <div className="crumbs">
            <span>{app.name}</span>
            <span aria-hidden="true">›</span>
            <strong>{title}</strong>
          </div>
          {model.can_create && (
            <button className="btn primary" onClick={() => open("new")}>
              New
            </button>
          )}
          <div className="search">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={`Search ${title.toLowerCase()}…`}
              aria-label="Search records"
            />
            {query && (
              <span className="facet">
                {query}
                <button onClick={() => setQuery("")} aria-label="Clear search">
                  ×
                </button>
              </span>
            )}
          </div>
          {view === "kanban" && (
            <label className="groupby">
              Group by
              <select value={groupBy} onChange={(e) => setGroupBy(e.target.value)}>
                {fields.map((f) => (
                  <option key={f.name} value={f.name}>
                    {label(f.name)}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="switcher" role="group" aria-label="View">
            <button aria-pressed={view === "list"} onClick={() => setView("list")}>
              List
            </button>
            <button
              aria-pressed={view === "kanban"}
              onClick={() => setView("kanban")}
              disabled={fields.length === 0}
            >
              Kanban
            </button>
          </div>
        </div>
        {withheld.length > 0 && (
          <p className="notice-bar">
            🛡 Withheld from you: {withheld.map((w) => label(w.split(".")[1] ?? w)).join(", ")}
          </p>
        )}
        <div className="content">
          {loading ? (
            <p className="hint pad">Loading…</p>
          ) : shown.length === 0 ? (
            <Empty
              title={title}
              canCreate={model.can_create}
              onNew={() => open("new")}
              searching={!!query}
            />
          ) : view === "kanban" && groupBy ? (
            <Kanban
              token={token}
              model={model}
              fields={fields}
              rows={shown}
              groupBy={groupBy}
              selectedId={route.id}
              onOpen={open}
              onMoved={load}
              notify={notify}
            />
          ) : (
            <List fields={fields} rows={shown} selectedId={route.id} onOpen={open} />
          )}
        </div>
        <footer className="countbar">
          {shown.length} {shown.length === 1 ? "record" : "records"}
        </footer>
      </section>
      {selected && (
        <RecordPanel
          key={route.id}
          token={token}
          model={model}
          fields={fields}
          row={selected === "new" ? null : selected}
          allRows={rows}
          title={title}
          onClose={() => go({ app: route.app, model: route.model, view: route.view })}
          onSaved={async (id) => {
            await load();
            go({ app: route.app, model: route.model, view: route.view, id });
          }}
          notify={notify}
        />
      )}
    </>
  );
}

function Empty({
  title,
  canCreate,
  onNew,
  searching,
}: {
  title: string;
  canCreate: boolean;
  onNew: () => void;
  searching: boolean;
}) {
  return (
    <div className="empty">
      <div className="empty-art" aria-hidden="true" />
      <h3>{searching ? "No matches" : `No ${title.toLowerCase()} yet`}</h3>
      <p className="hint">
        {searching ? "Try a different search." : "Records you create show up here."}
      </p>
      {canCreate && !searching && (
        <button className="btn primary" onClick={onNew}>
          Create the first one
        </button>
      )}
    </div>
  );
}

function Cell({ field, value }: { field: string; value: unknown }) {
  if (isState(field) && typeof value === "string" && value)
    return (
      <span className={`badge s-${value.replace(/[^a-z0-9]/gi, "")}`}>
        {value.replace(/_/g, " ")}
      </span>
    );
  if (isMasked(value)) return <span className="masked">{String(value)}</span>;
  return <>{display(field, value)}</>;
}

function List({
  fields,
  rows,
  selectedId,
  onOpen,
}: {
  fields: FieldMeta[];
  rows: Row[];
  selectedId: string;
  onOpen: (id: string) => void;
}) {
  return (
    <table className="list">
      <thead>
        <tr>
          {fields.map((f) => (
            <th key={f.name} className={isMoney(f.name) ? "num" : ""} title={f.description}>
              {label(f.name)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr
            key={r.id}
            aria-selected={r.id === selectedId}
            tabIndex={0}
            onClick={() => onOpen(r.id)}
            onKeyDown={(e) => e.key === "Enter" && onOpen(r.id)}
          >
            {fields.map((f) => (
              <td key={f.name} className={isMoney(f.name) ? "num" : ""}>
                <Cell field={f.name} value={r[f.name]} />
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Kanban(props: {
  token: string;
  model: ModelMeta;
  fields: FieldMeta[];
  rows: Row[];
  groupBy: string;
  selectedId: string;
  onOpen: (id: string) => void;
  onMoved: () => Promise<void>;
  notify: (t: string) => void;
}) {
  const { token, model, fields, rows, groupBy, selectedId, onOpen, onMoved, notify } = props;
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const columns = useMemo(() => {
    const seen: string[] = [];
    for (const r of rows) {
      const v = String(r[groupBy] ?? "");
      if (!seen.includes(v)) seen.push(v);
    }
    return seen;
  }, [rows, groupBy]);
  const title =
    fields.find((f) => !isState(f.name) && !isMoney(f.name) && f.name !== groupBy) ?? fields[0];
  const money1 = fields.find((f) => isMoney(f.name));
  const rest = fields.filter((f) => f.name !== title?.name && f.name !== groupBy).slice(0, 3);

  async function move(id: string, to: string) {
    const r = await call(token, `${model.name}.update`, { id, [groupBy]: to });
    if (r.kind === "error") notify(r.message);
    await onMoved();
  }
  const drop = (e: DragEvent, col: string) => {
    e.preventDefault();
    setOver(null);
    const id = e.dataTransfer.getData("text/plain");
    if (id && col !== "") void move(id, col);
  };

  return (
    <div className="kanban">
      {columns.map((col) => {
        const cards = rows.filter((r) => String(r[groupBy] ?? "") === col);
        const total = money1 ? cards.reduce((n, r) => n + (Number(r[money1.name]) || 0), 0) : null;
        return (
          <section
            key={col || "none"}
            className={`kcol${over === col ? " over" : ""}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOver(col);
            }}
            onDragLeave={() => setOver(null)}
            onDrop={(e) => drop(e, col)}
            aria-label={col || "No value"}
          >
            <header>
              <b>{(col || "No value").replace(/_/g, " ")}</b>
              <span className="count">{cards.length}</span>
              {total !== null && <span className="sum">{money(total)}</span>}
            </header>
            {cards.map((r) => (
              <article
                key={r.id}
                className={`kcard${r.id === selectedId ? " sel" : ""}${dragging === r.id ? " drag" : ""}`}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/plain", r.id);
                  setDragging(r.id);
                }}
                onDragEnd={() => setDragging(null)}
                onClick={() => onOpen(r.id)}
              >
                <strong>{title ? display(title.name, r[title.name]) : r.id.slice(0, 8)}</strong>
                {rest.map((f) => (
                  <div key={f.name} className="kline">
                    <span>{label(f.name)}</span>
                    <span>
                      <Cell field={f.name} value={r[f.name]} />
                    </span>
                  </div>
                ))}
                {/* Keyboard and touch twin of dragging */}
                <select
                  className="moveto"
                  aria-label="Move to"
                  value=""
                  onClick={(e) => e.stopPropagation()}
                  onChange={(e) => e.target.value && void move(r.id, e.target.value)}
                >
                  <option value="">Move to…</option>
                  {columns
                    .filter((c) => c !== col && c !== "")
                    .map((c) => (
                      <option key={c} value={c}>
                        {c.replace(/_/g, " ")}
                      </option>
                    ))}
                </select>
              </article>
            ))}
          </section>
        );
      })}
    </div>
  );
}
