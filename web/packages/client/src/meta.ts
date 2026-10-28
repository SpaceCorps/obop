/**
 * The metadata of `GET /api/meta`: the modules, models, readable fields and actions the caller's
 * token may see. The OpenAPI document describes the answer only as "The metadata.", so these types
 * are written by hand and kept loose on purpose: they name the keys a client relies on and let
 * every other key through as `unknown`. A package installed later adds models that no generated
 * type knows about; this is where a generic screen finds them.
 */

export interface ObopFieldMeta {
  name: string;
  description?: string;
  /** `text`, `longtext`, `number`, `money`, `date`, `datetime`, `decimal`, `bool`, `select`, `ref`, `image` or `file`. */
  kind: string;
  required?: boolean;
  /** Only for `select`: the allowed values. */
  options?: string[];
  /** Only for `ref`: the model the id points at. */
  to?: string;
  governed?: boolean;
  computed?: boolean;
  unique?: boolean;
  sequence?: boolean;
  [key: string]: unknown;
}

export interface ObopActionMeta {
  /** `<model>.<action>`. */
  id: string;
  description?: string;
  needs_approval?: boolean;
  [key: string]: unknown;
}

export interface ObopModelMeta {
  name: string;
  description?: string;
  can_create?: boolean;
  can_bulk_create?: boolean;
  can_update?: boolean;
  can_delete?: boolean;
  fields: ObopFieldMeta[];
  actions: ObopActionMeta[];
  [key: string]: unknown;
}

export interface ObopModuleMeta {
  id: string;
  name: string;
  models: ObopModelMeta[];
  [key: string]: unknown;
}

export interface ObopMeta {
  modules: ObopModuleMeta[];
  core_version?: string;
  /** The instance's time zone and the version of its pinned IANA database (D72). */
  time?: { zone?: string; tzdb_version?: string };
  [key: string]: unknown;
}
