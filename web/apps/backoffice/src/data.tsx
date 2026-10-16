import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
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
import type { App } from "./Chrome.tsx";
import { isEmpty } from "./format.ts";
import type { Lookup } from "./query.ts";

const NO_STRINGS: string[] = [];
const NO_ROWS: Row[] = [];

export type Tone = "ok" | "error" | "info";

/** Lists of records, loaded once per model and shared by the page, ref pickers, labels and smart buttons. */
class RowStore {
  private rows = new Map<string, Row[]>();
  private denied = new Map<string, string[]>();
  private errors = new Map<string, string>();
  private pending = new Map<string, Promise<void>>();
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

  load(model: string): Promise<void> {
    const running = this.pending.get(model);
    if (running) return running;
    if (!this.known(model)) {
      this.errors.set(model, "unknown model");
      return Promise.resolve();
    }
    const p = list(this.token, model).then((r) => {
      this.pending.delete(model);
      if ("error" in r) this.errors.set(model, r.error);
      else {
        this.errors.delete(model);
        this.rows.set(model, r.rows);
        this.denied.set(model, r.denied);
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
  notify: (text: string, tone?: Tone) => void;
  confirm: (message: string, confirmLabel?: string, cancelLabel?: string) => Promise<boolean>;
  /** The label of one record: the first text field, or the short id. */
  recordLabel: (model: string, row: Row) => string;
  refText: (model: string, id: string) => string;
  lookup: Lookup;
  /** Where a model (or one record, or a filtered list) can be opened, if this person can see it. */
  linkTo: (model: string, opts?: { id?: string; st?: string }) => string | null;
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
export function useRows(model: string) {
  const d = useData();
  const rows = d.store.peek(model);
  return {
    rows: rows ?? NO_ROWS,
    loaded: rows !== undefined,
    error: d.store.errorOf(model),
    denied: d.store.deniedOf(model),
    reload: () => d.store.load(model),
  };
}

export function labelField(model: ModelMeta): FieldMeta | undefined {
  return (
    model.fields.find((f) => f.kind === "text" && f.required) ??
    model.fields.find((f) => f.kind === "text" || f.kind === "longtext")
  );
}

export function DataProvider(props: {
  token: string;
  modules: ModuleMeta[];
  apps: App[];
  notify: (text: string, tone?: Tone) => void;
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
  const modelsRef = useRef(models);
  modelsRef.current = models;
  const store = useMemo(() => new RowStore(token, (n) => modelsRef.current.get(n)?.model), [token]);
  const version = useSyncExternalStore(store.subscribe, store.getVersion);

  // A reloaded module list (install, uninstall) means the cached rows may be stale.
  useEffect(() => {
    for (const name of models.keys()) if (store.peek(name) !== undefined) void store.load(name);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [models]);

  const value = useMemo<Ctx>(() => {
    const recordLabel = (model: string, row: Row) => {
      const m = models.get(model)?.model;
      const f = m && labelField(m);
      const v = f ? row[f.name] : undefined;
      return isEmpty(v) ? row.id.slice(0, 8) : String(v);
    };
    const refText = (model: string, id: string) => {
      const row = store.peek(model)?.find((r) => r.id === id);
      return row ? recordLabel(model, row) : id.slice(0, 8);
    };
    return {
      token,
      modules,
      apps,
      models,
      store,
      version,
      notify,
      confirm,
      recordLabel,
      refText,
      lookup: {
        refText,
        refIds: (model) => (store.peek(model) ?? []).map((r) => r.id),
      },
      linkTo: linkFor,
      run: async (id, body = {}) => {
        const r = await call(token, id, body);
        if (r.kind === "ok") await store.load(id.split(".")[0]!);
        return r;
      },
    };
  }, [token, modules, apps, models, store, version, notify, confirm, linkFor]);

  return <DataCtx.Provider value={value}>{props.children}</DataCtx.Provider>;
}
