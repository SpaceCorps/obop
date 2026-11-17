import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { fmt, getCurrency, inSentence, t } from "@obop/ui";
import { search, type FieldMeta, type ModelMeta, type Row } from "./api.ts";
import { collect, EXPORT_CAP } from "./bulk.ts";
import type { App } from "./Chrome.tsx";
import { ControlPanel, type ViewDef } from "./ControlPanel.tsx";
import { useData, useRows } from "./data.tsx";
import { loadFavorites, readPref, writePref, type Favorite } from "./favorites.ts";
import { defaultFilter } from "./filters.ts";
import { csvCell, download, plainText, rowCurrency, visibleFields } from "./format.ts";
import { ImportDialog } from "./ImportDialog.tsx";
import { fieldTitle, fieldTitleByName, modelTitle } from "./labels.ts";
import { PHONE_QUERY, useMedia } from "./media.ts";
import { pluralTitle } from "./plural.ts";
import { defaultView, listColumns, newDefaults } from "./presentation.ts";
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
import { RecordPanel, useMissingRecord } from "./RecordPanel.tsx";
import { today } from "./quickFilters.ts";
import { prefersFullPage } from "./recordLayout.ts";
import { indexModels } from "./presentation.ts";
import { isFull, type Go, type Route } from "./route.ts";
import { isWithheldError, onServer, toServer } from "./serverQuery.ts";
import { EmptyState, Skeleton } from "./ui.tsx";
import { useRecord, useServerRows } from "./useServer.ts";
import { dateFields } from "./views/CalendarView.tsx";
import { hasGallery } from "./views/GalleryView.tsx";
import { kanbanField } from "./views/KanbanView.tsx";
import { listOrder } from "./views/listLayout.ts";
import { VIEW_REGISTRY, viewOf } from "./views/registry.ts";
import type { ViewProps } from "./views/types.ts";
import { governedNames, moveRecord } from "./workflow.ts";

/** What a page that wraps ModelPage (a registration of extensions.ts) may do with it. */
export type ModelPageApi = {
  /** Open the new record form with some fields filled in. */
  openNew: (defaults?: Record<string, unknown>) => void;
};

type Props = {
  app: App;
  model: ModelMeta;
  route: Route;
  go: Go;
  /** Extra controls next to the search box (an app adds its own button). */
  extra?: ReactNode;
  /** Replaces the generic "New" form, for a record an app creates with its own guided flow. */
  onNew?: () => void;
  /**
   * Asked each time the page opens on a new record (`id=new`, also through `openNew`): the values
   * the form starts with (a file picked on another page, the person's own employee record), under
   * any value `openNew` was given (a day picked in the calendar). `null`: none.
   */
  takeNewDefaults?: () => Record<string, unknown> | null;
  /** Drawn after the page and its dialogs: an app's own dialog, which may open the new form. */
  after?: (page: ModelPageApi) => ReactNode;
  /**
   * What the page shows when the search finds nothing, instead of the generic "No matches" (an
   * empty review queue is good news). `null`: the generic one. Not asked while text is typed.
   */
  noMatch?: (state: ViewState) => ReactNode | null;
};

const VIEWS: Record<string, ViewDef> = Object.fromEntries(
  Object.values(VIEW_REGISTRY).map((v) => [v.id, { id: v.id, label: v.label, icon: v.icon }]),
);

const PAGE_SIZES = [25, 50, 100, 200];

function storedView(model: string): string {
  try {
    return localStorage.getItem(`obop-view-${model}`) ?? "";
  } catch {
    return "";
  }
}

/** The signed-in user name (App keeps it in the session), for "Mine" and `$user` defaults. */
function signedInUser(): string {
  try {
    return sessionStorage.getItem("obop-user") ?? "";
  } catch {
    return "";
  }
}

/** A single-value `is` facet on a ref: the parent a list of children is opened from. */
function parentFacet(facets: Cond[], fields: FieldMeta[]) {
  if (facets.length !== 1) return undefined;
  const c = facets[0]!;
  if (c.op !== "is" || c.v.length !== 1 || !c.v[0]) return undefined;
  const f = fields.find((x) => x.name === c.f);
  return f?.kind === "ref" && f.to ? { field: f, id: c.v[0] } : undefined;
}

export function ModelPage({
  app,
  model,
  route,
  go,
  extra: appExtra,
  onNew,
  takeNewDefaults,
  after,
  noMatch,
}: Props) {
  const d = useData();
  const title = modelTitle(model, app.module.id);
  const plural = pluralTitle(model, title);
  const user = useMemo(signedInUser, []);
  const phone = useMedia(PHONE_QUERY);
  const state = useMemo(() => decodeState(route.st), [route.st]);
  // The search text as it is typed (ControlPanel, 250 ms after the last key), before it becomes a
  // chip: it narrows the records at once, and is not part of the address.
  const [live, setLive] = useState("");
  const liveCond = useMemo<Cond | null>(() => (live ? { op: "search", v: live } : null), [live]);
  const query = useMemo<ViewState>(
    () => (liveCond ? { ...state, facets: [...state.facets, liveCond] } : state),
    [state, liveCond],
  );
  // The dialogs a view opens (bulk edit and delete) share one slot, which the list view rewrites
  // whenever it mounts. The import dialog is the page's own, so it has a slot of its own: the first
  // import into an empty model turns the empty state into the list, and that mount must not close it.
  const [extraDialog, setExtraDialog] = useState<ReactNode>(null);
  const [importing, setImporting] = useState(false);
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
    // Cards with the record's picture (Products, Contacts, Listings), or a package that asks for them.
    if (hasGallery(fields) || model.ui?.default_view === "gallery") ids.push("gallery");
    if (dateFields(fields).length) ids.push("calendar");
    if (canGroup) ids.push("pivot", "graph");
    // The week grid of a ref, a day and minutes (views/GridView.tsx), where the package asks for it.
    if (model.ui?.default_view === "grid") ids.push("grid");
    return ids;
  }, [fields, model]);
  const preferred = route.view || storedView(model.name);
  const view = available.includes(preferred) ? preferred : defaultView(model, available);

  // The list pages on the server (D49) whenever the server can answer the whole search. Kanban,
  // pivot, graph and calendar ask the server for counts, sums and pages (D4, `aggregate`) whenever
  // the filter is expressible. Anything else (an any-field search, a "contains" on a select, grouping
  // the list, a sort by a ref) loads the records, up to 5,000, and works in the browser. See
  // serverQuery.ts.
  // Without a sort of the person's own, the list starts in the model's default order (L2.4).
  const listDefault = useMemo(() => listOrder(model, fields), [model, fields]);
  // i18n-memo-ok: a range bound is read the same in every language and region (a dot or a comma is
  // taken as the decimal mark); the only reader of the i18n store is fmt.exponent(), the digits of
  // the instance currency, which a change of language or region does not change
  const plan = useMemo(() => toServer(query, fields, listDefault), [query, fields, listDefault]);
  // The search's order: the person's sort, else the list's default (listLayout.ts `listOrder`).
  const order = plan.order;
  const grouped = query.group.length > 0;
  const serverMode = view === "list" && onServer(plan, query);
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
    order: aggMode ? [] : order,
    offset: aggMode ? 0 : offset,
    limit: aggMode ? 1 : pageSize,
    // Only an order by text follows the language (the server collates it in the locale).
    collated:
      !aggMode &&
      order.some((o) => {
        const k = fields.find((f) => f.name === o.field)?.kind;
        return k === "text" || k === "longtext" || k === "select" || k === "ref";
      }),
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
    (
      patch: { view?: string; id?: string; st?: string; page?: string; full?: string },
      replace = false,
    ) => {
      const id = patch.id ?? route.id;
      go(
        {
          ...base,
          view: patch.view ?? route.view,
          id,
          st: patch.st ?? route.st,
          page: patch.page ?? route.page,
          // The record as a page (`&full=1`) stays while a record is open, also while the view or
          // the search changes; closing it goes back to the list.
          full: patch.full ?? (id ? route.full : ""),
        },
        { replace },
      );
    },
    [go, base, route.view, route.id, route.st, route.page, route.full],
  );
  // A new search starts on the first page.
  const setState = useCallback((s: ViewState) => put({ st: encodeState(s), page: "" }), [put]);
  // What a view hands back carries the live text as a facet: it stays out of the address.
  const setViewState = useCallback(
    (s: ViewState) =>
      setState(liveCond ? { ...s, facets: s.facets.filter((c) => c !== liveCond) } : s),
    [setState, liveCond],
  );
  // Typing narrows from the first page on, as a new search does.
  const onLive = useCallback(
    (text: string) => {
      if (text === live) return;
      setLive(text);
      if (route.page && route.page !== "1") put({ page: "" }, true);
    },
    [live, put, route.page],
  );

  // A favorite marked default applies when the model opens without any search of its own; else the
  // package's default filter (`ui.default_filter`), which the person removes like any other.
  useEffect(() => {
    if (route.st) return;
    const fav = loadFavorites(model.name).find((f) => f.isDefault);
    if (fav) {
      put({ st: encodeState(fav.state), view: fav.view }, true);
      return;
    }
    const preset = defaultFilter(model, fields, { user, today: today() });
    if (preset) put({ st: encodeState({ ...decodeState(""), facets: preset }) }, true);
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
        : sortRows(
            applyFacets(rows, query.facets, fields, d.lookup),
            query.sort,
            fields,
            d.lookup,
            listDefault,
          ),
    // lookup changes when ref labels load
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [serverMode, aggMode, rows, query.facets, query.sort, fields, listDefault, d.version, d.lang],
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

  /**
   * The values a new record starts with (UI plan 2.9, B5): what the caller gives (a kanban column,
   * a calendar day, a quick-create title) over the single-value filters of the search (the parent
   * a list of lines was opened from), over the defaults of presentation.ts `newDefaults` (today,
   * the instance currency, `active`, the package's `default` hints).
   */
  const startValues = useCallback(
    (given: Record<string, unknown> = {}) =>
      newDefaults(model, {
        facets: state.facets,
        route: given,
        user,
        instanceCurrency: getCurrency(),
      }),
    [model, state.facets, user],
  );
  const [newValues, setNewValues] = useState<Record<string, unknown>>(() =>
    route.id === "new" ? startValues(takeNewDefaults?.() ?? {}) : {},
  );
  // `id=new` reached without openNew (the palette, a link): start from the search all the same.
  const openedHere = useRef(route.id === "new");
  useEffect(() => {
    if (route.id !== "new") {
      openedHere.current = false;
      return;
    }
    if (!openedHere.current) setNewValues(startValues(takeNewDefaults?.() ?? {}));
    openedHere.current = true;
    // only when the route turns to a new record
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [route.id]);

  // A record with lines or many fields opens as a page (record form, plan 2.6).
  const asPage = useMemo(() => prefersFullPage(model, indexModels(d.modules)), [model, d.modules]);
  const open = (id: string) => put({ id, full: asPage ? "1" : "" });
  const close = useCallback(() => put({ id: "", full: "" }), [put]);
  const openNew = (given: Record<string, unknown> = {}) => {
    if (onNew) return onNew();
    openedHere.current = true;
    setNewValues(startValues({ ...takeNewDefaults?.(), ...given }));
    put({ id: "new", full: asPage ? "1" : "" });
  };
  const found = useRecord(model.name, route.id, serverMode || aggMode ? srv.rows : rows);
  const selected = route.id === "new" ? "new" : found;
  useMissingRecord(found, route.id, () => put({ id: "", full: "" }, true));

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
    const body = { ...patch };
    for (const f of model.fields) if (f.computed || f.sequence) delete body[f.name];
    const r = await d.run(`${model.name}.update`, { id, ...body });
    if (r.kind === "ok") return null;
    if (r.kind === "approval") return approval("change", r.planId);
    return serverMessage(r.message);
  };
  const create: ViewProps["create"] = async (body) => {
    // A quick create (a kanban card) starts from the same values as the full form.
    const clean = startValues(body);
    // A governed field starts at its first option; the server refuses anything else.
    for (const g of governed) delete clean[g];
    // Computed values and numbers are the server's.
    for (const f of model.fields) if (f.computed || f.sequence) delete clean[f.name];
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
        if (order.length) q.order = order;
        const r = await search(d.token, model.name, q);
        return "error" in r ? { error: r.error } : { rows: r.rows, total: r.total };
      },
      o.cap,
      o,
    );
  };

  /**
   * The phone's Export (its ⋯ menu): every record the search matches, with the list's columns.
   * In aggregate views the page holds no rows, so the records are fetched from the server.
   */
  const exportCsv = async () => {
    const got = !aggMode
      ? await fetchAll({ cap: EXPORT_CAP })
      : await collect(async (offset, limit) => {
          const q: Record<string, unknown> = { offset, limit, order };
          if (plan.filter.length) q.filter = plan.filter;
          const r = await search(d.token, model.name, q);
          return "error" in r ? { error: r.error } : { rows: r.rows, total: r.total };
        }, EXPORT_CAP);
    if ("error" in got) return d.notify(serverMessage(got.error), "error");
    const chosen = readPref<string[] | null>(`obop-list-cols:${model.name}`, null);
    const cols = chosen
      ? fields.filter((f) => chosen.includes(f.name))
      : listColumns(model, 1920, { fields });
    const head = cols.map((f) => csvCell(fieldTitle(f))).join(",");
    const body = got.rows.map((r) =>
      cols.map((f) => csvCell(plainText(f, r[f.name], d.refText, rowCurrency(r)))).join(","),
    );
    // Inside the phone host app no file was saved, and the app's notice has said so.
    if (!download(`${model.name}.csv`, [head, ...body].join("\r\n") + "\r\n")) return;
    d.notify(
      got.truncated
        ? t("Exported the first {count} of {total} records only", {
            count: fmt.integer(got.rows.length),
            total: fmt.integer(got.total),
          })
        : t("{count, plural, one {Exported # record} other {Exported # records}}", {
            count: got.rows.length,
          }),
    );
  };

  const vp: ViewProps = {
    model,
    title,
    fields,
    rows: shown,
    setDialog: setExtraDialog,
    // A board's cards and a calendar keep the server's order unless the search sorts them; the
    // list's default order (newest first) is the list's.
    server: aggMode ? { filter: plan.filter, order: query.sort ? plan.order : [] } : undefined,
    state: query,
    setState: setViewState,
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

  const View = viewOf(view).component;
  const kf = view === "kanban" ? kanbanField(fields, state.group[0]) : undefined;
  const loaded = serverMode || aggMode ? srv.loaded : store.loaded;
  const error = serverMode || aggMode ? srv.error : store.error;
  const withheldError = !!error && isWithheldError(error);
  const truncated = !serverMode && !aggMode && store.loaded && store.total > rows.length;
  const canImport = !!model.can_bulk_create;
  const parent = parentFacet(state.facets, fields);
  const clearSearch = () => {
    setLive("");
    setState({ ...state, facets: [] });
  };

  /** What the page shows when nothing matches: the first record, a child's first, or no match. */
  const empty = (): ReactNode => {
    if (query.facets.length === 0)
      return (
        <EmptyState
          icon="sparkle"
          title={t("No {model} yet", { model: inSentence(plural) })}
          hint={
            model.access
              ? t("You see only the records that are yours or shared with you.")
              : model.can_create
                ? canImport
                  ? t("Create the first one, or bring them in from a spreadsheet.")
                  : t("Create the first one to get started.")
                : t("Records you create show up here.")
          }
          action={
            model.can_create
              ? {
                  label: t("Create the first {model}", { model: inSentence(title) }),
                  onClick: () => openNew(),
                }
              : undefined
          }
          secondary={
            canImport
              ? { label: t("Import a CSV file"), onClick: () => setImporting(true) }
              : undefined
          }
        />
      );
    if (parent && !liveCond) {
      const owner = d.refText(parent.field.to!, parent.id);
      return (
        <EmptyState
          icon="plus"
          title={t("No {model} on {parent} yet", { model: inSentence(plural), parent: owner })}
          hint={t("Add the first one here; it is linked to {parent}.", { parent: owner })}
          action={
            model.can_create
              ? {
                  label: t("Add {model}", { model: inSentence(title) }),
                  onClick: () => openNew(),
                }
              : undefined
          }
          secondary={{
            label: t("Show all {model}", { model: inSentence(plural) }),
            onClick: clearSearch,
          }}
        />
      );
    }
    const own = live ? null : (noMatch?.(state) ?? null);
    if (own) return own;
    return (
      <EmptyState
        icon="search"
        title={t("No matches")}
        hint={
          live
            ? t("Nothing fits “{text}” with the current filters.", { text: live })
            : t("No record fits the current search.")
        }
        action={{ label: t("Clear filters"), onClick: clearSearch }}
      />
    );
  };

  return (
    <>
      <section className="main panel" aria-label={title}>
        <ControlPanel
          appName={app.name}
          title={title}
          heading={plural}
          model={model.name}
          meta={model}
          fields={fields}
          canCreate={model.can_create}
          onNew={() => openNew()}
          onImport={canImport ? () => setImporting(true) : undefined}
          onExport={() => void exportCsv()}
          state={state}
          setState={setState}
          lookup={d.lookup}
          views={available.map((v) => VIEWS[v]!)}
          view={view}
          setView={setView}
          applyFavorite={applyFavorite}
          count={total}
          onLive={onLive}
          user={user}
          extra={
            <>
              {kf && !phone && (
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
                onClick: () => {
                  setLive("");
                  put({ st: "", page: "" });
                },
              }}
            />
          ) : error ? (
            <EmptyState title={t("Could not load records")} hint={serverMessage(error)} />
          ) : !loaded ? (
            <Skeleton rows={8} cols={Math.min(fields.length, 5)} />
          ) : total === 0 &&
            // The week grid is where the first entry is typed: it is drawn empty.
            view !== "grid" &&
            (view !== "calendar" || query.facets.length === 0) ? (
            empty()
          ) : (
            <View {...vp} />
          )}
        </div>
      </section>
      {extraDialog}
      {after?.({ openNew })}
      {importing && (
        <ImportDialog model={model} title={title} onClose={() => setImporting(false)} />
      )}
      {selected && (
        <RecordPanel
          key={route.id === "new" ? `new-${JSON.stringify(newValues)}` : route.id}
          model={model}
          fields={fields}
          row={selected === "new" ? null : selected}
          defaults={newValues}
          withheld={denied}
          title={title}
          onClose={close}
          siblings={shown.map((r) => r.id)}
          onOpen={(id) => put({ id })}
          full={isFull(route)}
          onFull={(on) => put({ full: on ? "1" : "" }, true)}
          onDuplicate={(values) => openNew(values)}
          onSaved={(id) =>
            // A list in its default order shows the newest first: a new record is on the first page.
            route.id === "new" && paging && !state.sort && listDefault[0]?.dir === "desc"
              ? put({ id, page: "" })
              : put({ id })
          }
        />
      )}
    </>
  );
}
