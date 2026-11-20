/**
 * Where an app adds its own screens to the generic back office (F3 of the UI plan), so that an
 * app-specific screen is a new file plus a registration, never an edit of a shared file.
 *
 * Three registries:
 * - `recordExtensions[model]`: buttons and blocks inside the record panel of one model.
 * - `modelPages[model]`: a component that replaces the generic ModelPage for one model.
 * - `appPages[appId]`: sidebar entries and whole pages of one app (and where the app opens).
 *
 * A registration is a file in `src/registrations/` that calls the `register*` functions below when
 * it is imported. `registrations.ts` imports every file of that folder (an eager `import.meta.glob`),
 * so nothing has to list it. Keep a registration file small: put the screen itself in its own
 * file and load it with `lazy()` when it is big, because every registration is in the first bundle.
 *
 * The extension only draws: everything it does goes through the existing commands (OR-4), so the
 * CLI, REST and MCP can do the same.
 */
import type { ComponentType, ReactNode } from "react";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import type { Edit } from "./edit.ts";
import type { NextActions } from "./presentation.ts";
import type { App } from "./Chrome.tsx";
import type { Go, Route } from "./route.ts";

// ---- record panel ------------------------------------------------------------------------------

/** What an action button or a panel block of a saved record gets. */
export type RecordExtProps = {
  model: ModelMeta;
  /** The fields the person may read (what the form shows). */
  fields: FieldMeta[];
  row: Row;
  /** A command of the panel is running: buttons should be disabled. */
  busy: boolean;
  /** The value of a field as the form holds it now (the draft, else the record). */
  value?: (f: FieldMeta) => Edit;
  /**
   * Save what the person changed in the form, as the action row does before an action; false
   * when the save was refused (the form shows why) and the action should not run.
   */
  saveFirst?: () => Promise<boolean>;
};

/** What a section inside the form gets; `row` is null while a new record is being made. */
export type RecordSectionProps = {
  model: ModelMeta;
  fields: FieldMeta[];
  row: Row | null;
  creating: boolean;
  /** The person may write the record (create for a new one, update otherwise). */
  canEdit: boolean;
  busy: boolean;
};

export type RecordExtension = {
  /**
   * Buttons at the end of the action row of a saved record, before Delete. A model with one keeps
   * the action row on a phone even when it has no actions of its own.
   */
  actions?: ComponentType<RecordExtProps>[];
  /** Blocks between the action row and the form, for a saved record (Sign: the signing links). */
  panels?: ComponentType<RecordExtProps>[];
  /** Sections inside the form, after the fields (rendered by FormBody's `<SectionSlot>`). */
  sections?: ComponentType<RecordSectionProps>[];
  /**
   * Actions of the model (`purchase_order.receive`) whose generic button is not drawn, because one
   * of `actions` runs the same command with a screen of its own (a receipt with the quantities).
   */
  overrides?: string[];
  /**
   * Asked before the form saves a change to a saved record: a sentence when the change needs a yes
   * first (Mileage: marking a trip private removes its route for good), else null. The form asks
   * it with Save and Keep editing; Keep editing saves nothing.
   */
  confirmSave?: ((row: Row, changes: Record<string, unknown>) => string | null)[];
};

export const recordExtensions: Record<string, RecordExtension> = {};

/** Add to the record panel of `model`. Several registrations for one model are merged in order. */
export function registerRecordExtension(model: string, ext: RecordExtension): void {
  const had = recordExtensions[model] ?? {};
  recordExtensions[model] = {
    actions: [...(had.actions ?? []), ...(ext.actions ?? [])],
    panels: [...(had.panels ?? []), ...(ext.panels ?? [])],
    sections: [...(had.sections ?? []), ...(ext.sections ?? [])],
    overrides: [...(had.overrides ?? []), ...(ext.overrides ?? [])],
    confirmSave: [...(had.confirmSave ?? []), ...(ext.confirmSave ?? [])],
  };
}

const NONE: Required<RecordExtension> = {
  actions: [],
  panels: [],
  sections: [],
  overrides: [],
  confirmSave: [],
};

/** What `model` adds to its record panel; empty lists when nothing is registered. */
export function recordExtensionOf(model: string): Required<RecordExtension> {
  const e = recordExtensions[model];
  return e ? ({ ...NONE, ...e } as Required<RecordExtension>) : NONE;
}

/** The next actions without those an extension draws itself (`overrides`). */
export function withoutOverridden(a: NextActions, overrides: readonly string[]): NextActions {
  if (!overrides.length) return a;
  const keep = (c: Command) => !overrides.includes(c.id);
  const primary = a.primary && keep(a.primary) ? a.primary : undefined;
  return {
    ...(primary ? { primary } : {}),
    secondary: a.secondary.filter(keep),
    danger: a.danger.filter(keep),
  };
}

// ---- model pages -------------------------------------------------------------------------------

/** The props of ModelPage, which a model page gets too (and may pass on to ModelPage). */
export type ModelPageProps = {
  app: App;
  model: ModelMeta;
  route: Route;
  go: Go;
};

export const modelPages: Record<string, ComponentType<ModelPageProps>> = {};

/** Draw `model` with `page` instead of the generic ModelPage (a POS terminal, a reconcile screen). */
export function registerModelPage(model: string, page: ComponentType<ModelPageProps>): void {
  modelPages[model] = page;
}

export function modelPageOf(model: string): ComponentType<ModelPageProps> | undefined {
  return modelPages[model];
}

// ---- app pages ---------------------------------------------------------------------------------

/** What the shell knows when it asks an app for its pages. */
export type AppCtx = {
  app: App;
  route: Route;
  go: Go;
  token: string;
  /** The signed-in user name. */
  email: string;
  /** The metadata and the settings have loaded. */
  ready: boolean;
};

/** One entry of the sidebar. */
export type NavLink = {
  key: string;
  label: string;
  href: string;
  title?: string;
  current: boolean;
};

/** What an app adds to the shell for the route that is open. Every key may be left out. */
export type AppPagesResult = {
  /** Sidebar entries above the list of models (under no heading). */
  navBefore?: NavLink[];
  /** Sidebar entries after the list of models. */
  navAfter?: NavLink[];
  /** Sidebar entries right after the main models (before Reporting and Configuration). */
  navMain?: NavLink[];
  /** Replaces the whole sidebar body (the head with the app's icon and name stays). */
  nav?: ReactNode;
  /**
   * The page for the current route, drawn instead of the model page; `null` or absent: the
   * generic page. While it is set no model link is marked current.
   */
  page?: ReactNode | null;
  /**
   * The page drawn is the app's own overview, so the sidebar marks its Overview entry current as
   * it does on the generic overview. Absent: an app page marks no entry.
   */
  overview?: boolean;
};

export type AppPages = {
  /**
   * A React hook. The shell calls the hook of every registered app on every render, in
   * registration order, with `ctx` set only for the app that is open (`null` otherwise), so a hook
   * may itself call hooks unconditionally. Return `{}` when `ctx` is null.
   */
  use: (ctx: AppCtx | null) => AppPagesResult;
  /** Where the app opens from the launcher, instead of its first model; absent: the first model. */
  home?: (app: App) => string | undefined;
};

export const appPages: Record<string, AppPages> = {};

export function registerAppPages(appId: string, pages: AppPages): void {
  appPages[appId] = pages;
}

/** Where `app` opens, when its registration says (an address such as `#/dashboards/board`). */
export function appHomeOf(app: App): string | undefined {
  return appPages[app.id]?.home?.(app);
}

/**
 * The hook the shell calls once per render: what the open app adds (`{}` when it adds nothing).
 * Registrations are fixed once the modules are imported, so the hooks are called in the same order
 * on every render.
 */
export function useAppPages(ctx: AppCtx | null): AppPagesResult {
  let out: AppPagesResult = {};
  for (const [id, pages] of Object.entries(appPages)) {
    const r = pages.use(ctx && ctx.app.id === id ? ctx : null);
    if (ctx && ctx.app.id === id) out = r;
  }
  return out;
}
