import { useCallback, useEffect, useMemo, useState } from "react";
import type { ModelMeta } from "./api.ts";
import type { App } from "./Chrome.tsx";
import { ControlPanel, type ViewDef } from "./ControlPanel.tsx";
import { useData, useRows } from "./data.tsx";
import { loadFavorites, type Favorite } from "./favorites.ts";
import { isState, label, modelLabel, visibleFields } from "./format.ts";
import {
  applyFacets,
  decodeState,
  encodeState,
  groupable,
  sortRows,
  type Cond,
  type ViewState,
} from "./query.ts";
import { RecordPanel } from "./RecordPanel.tsx";
import type { Go, Route } from "./route.ts";
import { EmptyState, Skeleton } from "./ui.tsx";
import { CalendarView, dateFields } from "./views/CalendarView.tsx";
import { GraphView } from "./views/GraphView.tsx";
import { KanbanView, kanbanField } from "./views/KanbanView.tsx";
import { ListView } from "./views/ListView.tsx";
import { PivotView } from "./views/PivotView.tsx";
import type { ViewProps } from "./views/types.ts";

type Props = { app: App; model: ModelMeta; route: Route; go: Go };

const VIEWS: Record<string, ViewDef> = {
  list: { id: "list", label: "List", icon: "list" },
  kanban: { id: "kanban", label: "Kanban", icon: "kanban" },
  calendar: { id: "calendar", label: "Calendar", icon: "calendar" },
  pivot: { id: "pivot", label: "Pivot", icon: "pivot" },
  graph: { id: "graph", label: "Graph", icon: "graph" },
};

function storedView(model: string): string {
  try {
    return localStorage.getItem(`obop-view-${model}`) ?? "";
  } catch {
    return "";
  }
}

export function ModelPage({ app, model, route, go }: Props) {
  const d = useData();
  const { rows, loaded, error, denied } = useRows(model.name);
  // Fields withheld from this principal never come back, so they are left out of every view.
  const fields = useMemo(
    () => visibleFields(model.fields).filter((f) => !denied.includes(`${model.name}.${f.name}`)),
    [model, denied],
  );
  const [newDefaults, setNewDefaults] = useState<Record<string, unknown>>({});
  const title = modelLabel(model.name, app.module.id);
  const state = useMemo(() => decodeState(route.st), [route.st]);

  const available = useMemo(() => {
    const ids = ["list"];
    const canGroup = groupable(fields).length > 0;
    if (canGroup) ids.push("kanban");
    if (dateFields(fields).length) ids.push("calendar");
    if (canGroup) ids.push("pivot", "graph");
    return ids;
  }, [fields]);
  const preferred = route.view || storedView(model.name);
  const view = available.includes(preferred)
    ? preferred
    : fields.some(isState) && available.includes("kanban")
      ? "kanban"
      : "list";

  const base = useMemo(() => ({ app: route.app, model: route.model }), [route.app, route.model]);
  const put = useCallback(
    (patch: { view?: string; id?: string; st?: string }, replace = false) =>
      go(
        {
          ...base,
          view: patch.view ?? route.view,
          id: patch.id ?? route.id,
          st: patch.st ?? route.st,
        },
        { replace },
      ),
    [go, base, route.view, route.id, route.st],
  );
  const setState = useCallback((s: ViewState) => put({ st: encodeState(s) }), [put]);

  // A favorite marked default applies when the model opens without any search of its own.
  useEffect(() => {
    if (route.st) return;
    const fav = loadFavorites(model.name).find((f) => f.isDefault);
    if (fav) put({ st: encodeState(fav.state), view: fav.view }, true);
    // only on first open of this model
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setView = (v: string) => {
    try {
      localStorage.setItem(`obop-view-${model.name}`, v);
    } catch {
      /* ignore */
    }
    put({ view: v });
  };

  const filtered = useMemo(
    () => sortRows(applyFacets(rows, state.facets, fields, d.lookup), state.sort, fields, d.lookup),
    // lookup changes when ref labels load
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, state.facets, state.sort, fields, d.version],
  );

  const open = (id: string) => put({ id });
  const close = useCallback(() => put({ id: "" }), [put]);
  const openNew = (defaults: Record<string, unknown> = {}) => {
    setNewDefaults(defaults);
    put({ id: "new" });
  };
  const selected = route.id === "new" ? "new" : rows.find((r) => r.id === route.id);

  const update: ViewProps["update"] = async (id, patch) => {
    const r = await d.run(`${model.name}.update`, { id, ...patch });
    if (r.kind === "ok") return null;
    if (r.kind === "approval")
      return `Approval needed (plan ${r.planId.slice(0, 8)}); nothing changed yet`;
    return r.message;
  };
  const create: ViewProps["create"] = async (body) => {
    const r = await d.run(`${model.name}.create`, body);
    if (r.kind === "ok") {
      d.notify(`${title} created`);
      return null;
    }
    if (r.kind === "approval")
      return `Approval needed (plan ${r.planId.slice(0, 8)}); nothing created yet`;
    return r.message;
  };
  const drill = (conds: Cond[]) =>
    put({ view: "list", st: encodeState({ ...state, facets: [...state.facets, ...conds] }) });

  const applyFavorite = (f: Favorite) =>
    put({ st: encodeState(f.state), view: available.includes(f.view) ? f.view : view });

  const vp: ViewProps = {
    model,
    title,
    fields,
    rows: filtered,
    state,
    setState,
    selectedId: route.id,
    onOpen: open,
    canCreate: model.can_create,
    canUpdate: model.can_update,
    update,
    create,
    newWith: openNew,
    drill,
  };

  const filtering = state.facets.length > 0;
  const kf = view === "kanban" ? kanbanField(fields, state.group[0]) : undefined;

  return (
    <>
      <section className="main panel" aria-label={title}>
        <ControlPanel
          appName={app.name}
          title={title}
          model={model.name}
          fields={fields}
          canCreate={model.can_create}
          onNew={() => openNew()}
          state={state}
          setState={setState}
          lookup={d.lookup}
          views={available.map((v) => VIEWS[v]!)}
          view={view}
          setView={setView}
          applyFavorite={applyFavorite}
          count={filtered.length}
          extra={
            kf && (
              <label className="inline-field">
                <span>Group by</span>
                <select
                  value={kf.name}
                  onChange={(e) => setState({ ...state, group: [e.target.value] })}
                >
                  {groupable(fields).map((f) => (
                    <option key={f.name} value={f.name}>
                      {label(f.name)}
                      {f.kind === "date" ? " (month)" : ""}
                    </option>
                  ))}
                </select>
              </label>
            )
          }
        />
        {denied.length > 0 && (
          <p className="notice-bar">
            Withheld from you: {denied.map((w) => label(w.split(".")[1] ?? w)).join(", ")}
          </p>
        )}
        <div className={`content view-${view}`}>
          {error ? (
            <EmptyState title="Could not load records" hint={error} />
          ) : !loaded ? (
            <Skeleton rows={8} cols={Math.min(fields.length, 5)} />
          ) : rows.length === 0 ? (
            <EmptyState
              title={`No ${title.toLowerCase()} yet`}
              hint="Records you create show up here."
              action={
                model.can_create
                  ? { label: `Create the first ${title.toLowerCase()}`, onClick: () => openNew() }
                  : undefined
              }
            />
          ) : filtered.length === 0 && view !== "calendar" ? (
            <EmptyState
              title="No matches"
              hint="No record fits the current search."
              action={
                filtering
                  ? { label: "Clear filters", onClick: () => setState({ ...state, facets: [] }) }
                  : undefined
              }
            />
          ) : view === "kanban" ? (
            <KanbanView {...vp} />
          ) : view === "pivot" ? (
            <PivotView {...vp} />
          ) : view === "graph" ? (
            <GraphView {...vp} />
          ) : view === "calendar" ? (
            <CalendarView {...vp} />
          ) : (
            <ListView {...vp} />
          )}
        </div>
      </section>
      {selected && (
        <RecordPanel
          key={route.id === "new" ? `new-${JSON.stringify(newDefaults)}` : route.id}
          model={model}
          fields={fields}
          row={selected === "new" ? null : selected}
          defaults={newDefaults}
          withheld={denied}
          title={title}
          onClose={close}
          onSaved={(id) => put({ id })}
        />
      )}
    </>
  );
}
