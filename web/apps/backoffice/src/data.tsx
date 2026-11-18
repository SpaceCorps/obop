import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import {
  call,
  list,
  type FieldMeta,
  type ModelMeta,
  type ModuleMeta,
  type Reply,
  type Row,
} from "./api.ts";
import { getCurrency, t, useI18n, type ToastAction } from "@obop/ui";
import type { App } from "./Chrome.tsx";
import { isEmpty, shownLabel } from "./format.ts";
import { titleOf } from "./presentation.ts";
import { registerMeta } from "./serverMessage.ts";
import type { Lookup } from "./query.ts";

const NO_STRINGS: string[] = [];
const NO_ROWS: Row[] = [];

/**
 * Models the server computes from another model's records: a write to the key changes what the
 * listed models read, so their cached rows are loaded again with it (#109: a provider policy
 * saved from any form enables or locks its provider in the connectors catalogue).
 */
export const DERIVED: Readonly<Record<string, readonly string[]>> = {
  provider_policy: ["connector_provider"],
};

/** The models whose cached rows a write to `model` makes stale: itself and what derives from it. */
export const staleAfter = (model: string): string[] => [model, ...(DERIVED[model] ?? [])];

export type Tone = "ok" | "error" | "info";

/** Lists of records, loaded once per model and shared by the page, ref pickers, labels and smart buttons. */
export class RowStore {
  private rows = new Map<string, Row[]>();
  private denied = new Map<string, string[]>();
  private errors = new Map<string, string>();
  private totals = new Map<string, number>();
  private pending = new Map<string, Promise<void>>();
  /** A load asked for while one was running, which starts when that one is done. */
  private queued = new Map<string, Promise<void>>();
  private listeners = new Set<() => void>();
  version = 0;

  constructor(
    private token: string,
    private known: (model: string) => ModelMeta | undefined,
  ) {}

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };
  getVersion = () => this.version;
  private bump() {
    this.version++;
    for (const l of this.listeners) l();
  }

  /** Cached rows without starting a load. */
  cached(model: string): Row[] | undefined {
    return this.rows.get(model);
  }

  /** Cached rows; the first call starts the load and returns `undefined` until it lands. */
  peek(model: string): Row[] | undefined {
    if (!this.rows.has(model) && !this.pending.has(model) && !this.errors.has(model))
      queueMicrotask(() => void this.load(model));
    return this.rows.get(model);
  }
  deniedOf(model: string): string[] {
    return this.denied.get(model) ?? NO_STRINGS;
  }
  errorOf(model: string): string | undefined {
    return this.errors.get(model);
  }
  /** How many records the model has in all; larger than the cached rows when it was capped. */
  totalOf(model: string): number {
    return this.totals.get(model) ?? this.rows.get(model)?.length ?? 0;
  }
  /**
   * Reload a model only if some view already holds its rows. The answer is read after the call:
   * a load that was already running may have been answered before a write that asks for this
   * refresh (a line saved while its order was loading, a record created while the list was), so
   * another load follows it instead of sharing its answer (FLAKY2: a total stayed 0.00).
   */
  refresh(model: string): Promise<void> {
    if (!this.rows.has(model)) return Promise.resolve();
    const running = this.pending.get(model);
    if (!running) return this.load(model);
    let next = this.queued.get(model);
    if (!next) {
      next = running.then(() => {
        this.queued.delete(model);
        return this.load(model);
      });
      this.queued.set(model, next);
    }
    return next;
  }

  /** Reload every cached model, and try again the ones whose load failed. */
  reloadAll() {
    for (const name of [...this.rows.keys(), ...this.errors.keys()]) {
      this.errors.delete(name);
      void this.load(name);
    }
  }

  load(model: string): Promise<void> {
    const running = this.pending.get(model);
    if (running) return running;
    if (!this.known(model)) {
      this.errors.set(model, t("Unknown model"));
      return Promise.resolve();
    }
    const p = list(this.token, model).then((r) => {
      this.pending.delete(model);
      if ("error" in r) this.errors.set(model, r.error);
      else {
        this.errors.delete(model);
        this.rows.set(model, r.rows);
        this.denied.set(model, r.denied);
        this.totals.set(model, r.total);
      }
      this.bump();
    });
    this.pending.set(model, p);
    return p;
  }
}

export type ModelInfo = { model: ModelMeta; module: ModuleMeta };

type Ctx = {
  token: string;
  modules: ModuleMeta[];
  apps: App[];
  models: Map<string, ModelInfo>;
  store: RowStore;
  version: number;
  /**
   * The language the labels of records (recordLabel, refText) are made in. They follow it when a label
   * field shows an option (shows_option_of), so a memo that keeps one lists it among its dependencies.
   */
  lang: string;
  /** Counts successful writes, so server-side queries know when to ask again. */
  mutations: number;
  /** A toast; `opts.action` adds a link or a button at its end ("Open" the record an action made). */
  notify: (text: string, tone?: Tone, opts?: { action?: ToastAction }) => void;
  confirm: (message: string, confirmLabel?: string, cancelLabel?: string) => Promise<boolean>;
  /** The name of one record, as its form names it (presentation.ts titleOf). */
  recordLabel: (model: string, row: Row) => string;
  refText: (model: string, id: string) => string;
  lookup: Lookup;
  /** Where a model (or one record, or a filtered list) can be opened, if this person can see it. */
  linkTo: (model: string, opts?: { id?: string; st?: string }) => string | null;
  /** After a bulk run: reload the cached rows of a model and make server-side views ask again. */
  changed: (model: string) => Promise<void>;
  /** Ask again for everything on screen (after a lost connection came back). */
  refreshAll: () => void;
  /** Run a command, then refresh the cached list of its model. */
  run: (id: string, body?: object) => Promise<Reply>;
};

const DataCtx = createContext<Ctx | null>(null);

export function useData(): Ctx {
  const c = useContext(DataCtx);
  if (!c) throw new Error("DataProvider is missing");
  return c;
}

/** The rows of a model, loaded on first use and refreshed after writes. */
export function useRows(model: string, enabled = true) {
  const d = useData();
  const rows = enabled ? d.store.peek(model) : d.store.cached(model);
  return {
    rows: rows ?? NO_ROWS,
    loaded: rows !== undefined,
    error: d.store.errorOf(model),
    denied: d.store.deniedOf(model),
    reload: () => d.store.load(model),
    /** All records of the model, which is more than `rows` when the load was capped. */
    total: d.store.totalOf(model),
  };
}

export function labelField(model: ModelMeta): FieldMeta | undefined {
  return (
    model.fields.find((f) => f.kind === "text" && f.required) ??
    // A numbered record (an invoice) is known by its number, not by whichever text field comes first.
    model.fields.find((f) => f.kind === "text" && f.sequence) ??
    model.fields.find((f) => f.kind === "text" || f.kind === "longtext")
  );
}

/**
 * The label of one record: its label field, or the short id. A label field that shows an option
 * (shows_option_of) names the record by that option's label, in the language active now, so the
 * answer for one record changes when the language does.
 */
export function recordLabelOf(models: Map<string, ModelInfo>, model: string, row: Row): string {
  const m = models.get(model)?.model;
  const f = m && labelField(m);
  const v = f ? row[f.name] : undefined;
  return isEmpty(v) ? row.id.slice(0, 8) : ((f && shownLabel(f, v, row)) ?? String(v));
}

export function DataProvider(props: {
  token: string;
  modules: ModuleMeta[];
  apps: App[];
  notify: Ctx["notify"];
  confirm: Ctx["confirm"];
  linkFor: Ctx["linkTo"];
  children: ReactNode;
}) {
  const { token, modules, apps, notify, confirm, linkFor } = props;
  const models = useMemo(() => {
    const m = new Map<string, ModelInfo>();
    for (const module of modules)
      for (const model of module.models) m.set(model.name, { model, module });
    return m;
  }, [modules]);
  registerMeta(modules);
  const modelsRef = useRef(models);
  modelsRef.current = models;
  const store = useMemo(() => new RowStore(token, (n) => modelsRef.current.get(n)?.model), [token]);
  const version = useSyncExternalStore(store.subscribe, store.getVersion);
  const [mutations, setMutations] = useState(0);
  // The label of a record (recordLabel, refText) is in the person's language when its label field
  // shows an option (shows_option_of). The context is a new object when the language changes, and a
  // memo that keeps such a label lists `d.lang` among its dependencies.
  const { lang } = useI18n();

  // A reloaded module list (install, uninstall) means the cached rows may be stale.
  useEffect(() => {
    for (const name of models.keys()) if (store.cached(name) !== undefined) void store.load(name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [models]);

  const value = useMemo<Ctx>(() => {
    // A record is named the way its form names it (titleOf): "1400 Receivables", the vendor's
    // party name, "Showroom counter · Nov 14", in lists, cards, subtitles, exports and chips alike.
    // A title that names another record follows at most a few steps, so a cycle cannot loop.
    const labelAt = (model: string, row: Row, depth: number): string => {
      const info = models.get(model);
      if (!info || depth > 3) return recordLabelOf(models, model, row);
      return titleOf(info.model, row, {
        refText: (m, id) => refTextAt(m, id, depth + 1),
        rowOf: (m, id) => store.peek(m)?.find((r) => r.id === id),
        modelOf: (m) => models.get(m)?.model,
        instanceCurrency: getCurrency(),
        moduleId: info.module.id,
      });
    };
    const refTextAt = (model: string, id: string, depth: number) => {
      const row = store.peek(model)?.find((r) => r.id === id);
      return row ? labelAt(model, row, depth) : id.slice(0, 8);
    };
    const recordLabel = (model: string, row: Row) => labelAt(model, row, 0);
    const refText = (model: string, id: string) => refTextAt(model, id, 0);
    return {
      token,
      modules,
      apps,
      models,
      store,
      version,
      lang,
      mutations,
      notify,
      confirm,
      recordLabel,
      refText,
      lookup: {
        refText,
        refIds: (model) => (store.peek(model) ?? []).map((r) => r.id),
      },
      linkTo: linkFor,
      changed: async (model) => {
        await Promise.all(staleAfter(model).map((m) => store.refresh(m)));
        setMutations((n) => n + 1);
      },
      refreshAll: () => {
        store.reloadAll();
        setMutations((n) => n + 1);
      },
      run: async (id, body = {}) => {
        const r = await call(token, id, body);
        if (r.kind === "ok") {
          // Actions and writes change records of their own model, and a delete may free others.
          await Promise.all(staleAfter(id.split(".")[0]!).map((m) => store.refresh(m)));
          setMutations((n) => n + 1);
        }
        return r;
      },
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token, modules, apps, models, store, version, mutations, notify, confirm, linkFor, lang]);

  return <DataCtx.Provider value={value}>{props.children}</DataCtx.Provider>;
}
