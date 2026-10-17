import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { fmt, inSentence, msg, t } from "@obop/ui";
import { search, type ModelMeta, type Row } from "./api.ts";
import { collect } from "./bulk.ts";
import type { App } from "./Chrome.tsx";
import { ControlPanel, type ViewDef } from "./ControlPanel.tsx";
import { useData, useRows } from "./data.tsx";
import { loadFavorites, readPref, writePref, type Favorite } from "./favorites.ts";
import { isState, visibleFields } from "./format.ts";
import { fieldTitle, fieldTitleByName, modelTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
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
import { isWithheldError, onServer, toServer } from "./serverQuery.ts";
import { EmptyState, Skeleton } from "./ui.tsx";
import { useRecord, useServerRows } from "./useServer.ts";
import { CalendarView, dateFields } from "./views/CalendarView.tsx";
import { GraphView } from "./views/GraphView.tsx";
import { KanbanView, kanbanField } from "./views/KanbanView.tsx";
import { ListView } from "./views/ListView.tsx";
import { PivotView } from "./views/PivotView.tsx";
import type { ViewProps } from "./views/types.ts";
import { governedNames, moveRecord } from "./workflow.ts";

type Props = {
  app: App;
  model: ModelMeta;
  route: Route;
  go: Go;
  /** Extra controls next to the search box (an app adds its own button). */
  extra?: ReactNode;
  /** Replaces the generic "New" form, for a record an app creates with its own guided flow. */
  onNew?: () => void;
};

const VIEWS: Record<string, ViewDef> = {
  list: { id: "list", label: msg("List"), icon: "list" },
  kanban: { id: "kanban", label: msg("Kanban"), icon: "kanban" },
  calendar: { id: "calendar", label: msg("Calendar"), icon: "calendar" },
  pivot: { id: "pivot", label: msg("Pivot"), icon: "pivot" },
  graph: { id: "graph", label: msg("Graph"), icon: "graph" },
};

const PAGE_SIZES = [25, 50, 100, 200];

function storedView(model: string): string {
  try {
    return localStorage.getItem(`obop-view-${model}`) ?? "";
  } catch {
    return "";
  }
}

export function ModelPage({ app, model, route, go, extra: appExtra, onNew }: Props) {
  const d = useData();
  const title = modelTitle(model, app.module.id);
  const state = useMemo(() => decodeState(route.st), [route.st]);
  const [newDefaults, setNewDefaults] = useState<Record<string, unknown>>({});
  const [pageSize, setPageSizeState] = useState(() => {
    const n = readPref<number>("obop-pagesize", 100);
    return PAGE_SIZES.includes(n) ? n : 100;
  });

  // Fields withheld from this principal never come back, so they are left out of every view.
  // They are learned from whichever answer arrives: a search page or the full load.
  const [serverDenied, setServerDenied] = useState<string[]>([]);
  const [cachedDenied, setCachedDenied] = useState<string[]>([]);
  const denied = useMemo(
    () => [...new Set([...serverDenied, ...cachedDenied])],
    [serverDenied, cachedDenied],
  );
  const fields = useMemo(
    () => visibleFields(model.fields).filter((f) => !denied.includes(`${model.name}.${f.name}`)),
    [model, denied],
  );

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

  // The list pages on the server (D49) whenever the server can answer the whole search. Kanban,
  // pivot, graph and calendar ask the server for counts, sums and pages (D4, `aggregate`) whenever
  // the filter is expressible. Anything else (an any-field search, a "contains" on a select, grouping
  // the list, a sort by a ref) loads the records, up to 5,000, and works in the browser. See
  // serverQuery.ts.
  const plan = useMemo(() => toServer(state, fields), [state, fields]);
  const grouped = state.group.length > 0;
  const serverMode = view === "list" && onServer(plan, state);
  const aggMode = view !== "list" && plan.rest.length === 0;
  const page = Math.max(1, parseInt(route.page, 10) || 1);
  const offset = (page - 1) * pageSize;

  const store = useRows(model.name, !serverMode && !aggMode);
  // In aggregate mode this asks for one record: it answers how many records match and which
  // fields are withheld; the view asks for the rest.
  const srv = useServerRows({
    enabled: serverMode || aggMode,
    model: model.name,
    filter: plan.filter,
    order: aggMode ? [] : plan.order,
    offset: aggMode ? 0 : offset,
    limit: aggMode ? 1 : pageSize,
  });
  useEffect(() => {
    if (srv.denied.length || serverDenied.length) setServerDenied(srv.denied);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [srv.denied]);
  useEffect(() => {
    if (store.denied.length || cachedDenied.length) setCachedDenied([...store.denied]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [store.denied]);

  const base = useMemo(() => ({ app: route.app, model: route.model }), [route.app, route.model]);
  const put = useCallback(
    (patch: { view?: string; id?: string; st?: string; page?: string }, replace = false) =>
      go(
        {
          ...base,
          view: patch.view ?? route.view,
          id: patch.id ?? route.id,
          st: patch.st ?? route.st,
          page: patch.page ?? route.page,
        },
        { replace },
      ),
    [go, base, route.view, route.id, route.st, route.page],
  );
  // A new search starts on the first page.
  const setState = useCallback((s: ViewState) => put({ st: encodeState(s), page: "" }), [put]);

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
    put({ view: v, page: "" });
  };

  const rows = store.rows;
  const filtered = useMemo(
    () =>
      serverMode || aggMode
        ? []
        : sortRows(applyFacets(rows, state.facets, fields, d.lookup), state.sort, fields, d.lookup),
    // lookup changes when ref labels load
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [serverMode, aggMode, rows, state.facets, state.sort, fields, d.version],
  );
  const paging = view === "list" && !grouped;
  const shown: Row[] = aggMode
    ? []
    : serverMode
      ? srv.rows
      : paging
        ? filtered.slice(offset, offset + pageSize)
        : filtered;
  const total = serverMode || aggMode ? srv.total : filtered.length;

  // Deleting the last record of a page leaves an empty page: go back to the last one.
  useEffect(() => {
    if (!paging || page === 1) return;
    const settled = serverMode ? srv.loaded && !srv.loading : store.loaded;
    if (settled && total > 0 && offset >= total)
      put({ page: String(Math.max(1, Math.ceil(total / pageSize))) }, true);
  }, [
    paging,
    page,
    serverMode,
    srv.loaded,
    srv.loading,
    store.loaded,
    total,
    offset,
    pageSize,
    put,
  ]);

  const open = (id: string) => put({ id });
  const close = useCallback(() => put({ id: "" }), [put]);
  const openNew = (defaults: Record<string, unknown> = {}) => {
    if (onNew) return onNew();
    setNewDefaults(defaults);
    put({ id: "new" });
  };
  const found = useRecord(model.name, route.id, serverMode || aggMode ? srv.rows : rows);
  const selected = route.id === "new" ? "new" : found;

  const governed = useMemo(() => governedNames(model), [model]);
  const approval = (what: "change" | "create", planId: string) => {
    const plan = planId.slice(0, 8);
    return what === "change"
      ? t(
          "Approval needed: the change was proposed as plan {plan}. Nothing changes until a human approves it.",
          { plan },
        )
      : t(
          "Approval needed: creating the record was proposed as plan {plan}. Nothing changes until a human approves it.",
          { plan },
        );
  };
  const update: ViewProps["update"] = async (id, patch) => {
    const r = await d.run(`${model.name}.update`, { id, ...patch });
    if (r.kind === "ok") return null;
    if (r.kind === "approval") return approval("change", r.planId);
    return serverMessage(r.message);
  };
  const create: ViewProps["create"] = async (body) => {
    // A governed field starts at its first option; the server refuses anything else.
    const clean = { ...body };
    for (const g of governed) delete clean[g];
    const r = await d.run(`${model.name}.create`, clean);
    if (r.kind === "ok") {
      d.notify(t("{model} created", { model: title }));
      return null;
    }
    if (r.kind === "approval") return approval("create", r.planId);
    return serverMessage(r.message);
  };
  const moveState: ViewProps["moveState"] = async (id, field, to) => {
    const f = model.fields.find((x) => x.name === field);
    const row = (serverMode || aggMode ? srv.rows : rows).find((r) => r.id === id);
    if (!f || !row) return t("Record not found");
    const out = await moveRecord(d.run, model, f, row, to, title);
    return out.kind === "ok" ? null : serverMessage(out.message);
  };
  const drill = (conds: Cond[]) =>
    put({
      view: "list",
      st: encodeState({ ...state, facets: [...state.facets, ...conds] }),
      page: "",
    });

  const applyFavorite = (f: Favorite) =>
    put({ st: encodeState(f.state), view: available.includes(f.view) ? f.view : view, page: "" });

  /** Every record the search matches, fetched page by page up to the cap (D1). */
  const fetchAll: ViewProps["fetchAll"] = async (o) => {
    if (!serverMode) {
      const rows = filtered.slice(0, o.cap);
      return {
        rows,
        total: filtered.length,
        truncated: filtered.length > rows.length,
        cancelled: false,
      };
    }
    return collect(
      async (offset, limit) => {
        const q: Record<string, unknown> = { offset, limit };
        if (plan.filter.length) q.filter = plan.filter;
        if (plan.order.length) q.order = plan.order;
        const r = await search(d.token, model.name, q);
        return "error" in r ? { error: r.error } : { rows: r.rows, total: r.total };
      },
      o.cap,
      o,
    );
  };

  const vp: ViewProps = {
    model,
    title,
    fields,
    rows: shown,
    server: aggMode ? { filter: plan.filter, order: plan.order } : undefined,
    state,
    setState,
    selectedId: route.id,
    onOpen: open,
    canCreate: model.can_create,
    canUpdate: model.can_update,
    canDelete: model.can_delete,
    update,
    create,
    moveState,
    newWith: openNew,
    drill,
    total,
    fetchAll,
    pager: paging
      ? {
          offset,
          limit: pageSize,
          busy: serverMode && srv.loading,
          setOffset: (o) => put({ page: String(Math.floor(o / pageSize) + 1) }),
          setLimit: (n) => {
            setPageSizeState(n);
            writePref("obop-pagesize", n);
            put({ page: "" });
          },
        }
      : undefined,
  };

  const filtering = state.facets.length > 0;
  const kf = view === "kanban" ? kanbanField(fields, state.group[0]) : undefined;
  const loaded = serverMode || aggMode ? srv.loaded : store.loaded;
  const error = serverMode || aggMode ? srv.error : store.error;
  const withheldError = !!error && isWithheldError(error);
  const modelEmpty =
    serverMode || aggMode
      ? srv.loaded && !srv.error && srv.total === 0 && plan.filter.length === 0
      : store.loaded && rows.length === 0;
  const truncated = !serverMode && !aggMode && store.loaded && store.total > rows.length;

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
          count={total}
          extra={
            <>
              {kf && (
                <label className="inline-field">
                  <span>{t("Group by")}</span>
                  <select
                    value={kf.name}
                    onChange={(e) => setState({ ...state, group: [e.target.value] })}
                  >
                    {groupable(fields).map((f) => (
                      <option key={f.name} value={f.name}>
                        {f.kind === "date"
                          ? t("{field} (month)", { field: fieldTitle(f) })
                          : fieldTitle(f)}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {appExtra}
            </>
          }
        />
        {denied.length > 0 && (
          <p className="notice-bar">
            {t("Withheld from you: {fields}", {
              fields: denied
                .map((w) => fieldTitleByName(w.split(".")[1] ?? w, model.fields))
                .join(", "),
            })}
          </p>
        )}
        {truncated && (
          <p className="notice-bar" role="status">
            {t(
              "Showing the first {shown} of {total} records. Narrow the search, or use the list, which pages through all of them.",
              { shown: fmt.integer(rows.length), total: fmt.integer(store.total) },
            )}
          </p>
        )}
        <div className={`content view-${view}`}>
          {withheldError ? (
            <EmptyState
              title={t("This search uses a field you cannot read")}
              hint={serverMessage(error)}
              action={{
                label: t("Clear filters and sorting"),
                onClick: () => put({ st: "", page: "" }),
              }}
            />
          ) : error ? (
            <EmptyState title={t("Could not load records")} hint={serverMessage(error)} />
          ) : !loaded ? (
            <Skeleton rows={8} cols={Math.min(fields.length, 5)} />
          ) : modelEmpty ? (
            <EmptyState
              title={t("No {model} yet", { model: inSentence(title) })}
              hint={t("Records you create show up here.")}
              action={
                model.can_create
                  ? {
                      label: t("Create the first {model}", { model: inSentence(title) }),
                      onClick: () => openNew(),
                    }
                  : undefined
              }
            />
          ) : total === 0 && view !== "calendar" ? (
            <EmptyState
              title={t("No matches")}
              hint={t("No record fits the current search.")}
              action={
                filtering
                  ? { label: t("Clear filters"), onClick: () => setState({ ...state, facets: [] }) }
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
          onSaved={(id) =>
            // A new record sits at the end of an unsorted list: show the page it is on.
            route.id === "new" && paging && !state.sort
              ? put({ id, page: String(Math.max(1, Math.ceil((total + 1) / pageSize))) })
              : put({ id })
          }
        />
      )}
    </>
  );
}
