// What the server returns for the Modules page (D55): `module.list`, `marketplace_listing.list`
// and `module_plan.preview`. Everything after `source` is metadata from the module's obop.toml
// (or declared in code for a built-in module) plus facts computed against this instance.

export type Maturity = "stable" | "beta" | "experimental" | "";
export type LicenseKind = "spdx" | "custom" | "proprietary" | "none";
export type Source = "builtin" | "upload" | "marketplace";

/** How one dependency stands today. The page derives the status chip from these facts. */
export type Dep = {
  id: string;
  name: string;
  /** A semver requirement such as ">=0.3, <1" or "^0.2". */
  requirement: string;
  /** `built-in` modules version with the core. */
  kind: "built-in" | "package" | "missing";
  installed_version: string | null;
  satisfied: boolean;
  available_version: string | null;
  available_satisfies: boolean;
  optional: boolean;
};

export type Blocker = { module: string; kind: string; message: string };

export type ContentsAction = { name: string; description: string; approval: boolean };
export type ContentsModel = {
  name: string;
  description: string;
  fields: number;
  personal_fields: number;
  read_group: string;
  write_group: string;
  actions: ContentsAction[];
};
export type Contents = {
  models: ContentsModel[];
  groups: string[];
  approval_steps: { model: string; action: string; group: string }[];
  data_classes: Record<string, number>;
  fields: number;
  personal_data_fields: number;
};

/** A module as the page lists it: installed (built in, upload or marketplace) or offered. */
export type Row = {
  id: string;
  name: string;
  version: string;
  description: string;
  summary: string;
  source: Source;
  author: string;
  authors: string[];
  license: string;
  license_kind: LicenseKind;
  license_url: string | null;
  homepage: string | null;
  repository: string | null;
  documentation: string | null;
  support: string | null;
  keywords: string[];
  categories: string[];
  icon: string;
  maturity: Maturity;
  requires_core: string;
  compatible: boolean;
  dependencies: Dep[];
  dependents: string[];
  update_available: string | null;
  models: string[];
  groups: string[];
  /** Installed rows only. */
  contents?: Contents;
  installed_at?: number | null;
  installed_by?: string | null;
  uninstallable?: boolean;
  uninstall_blockers?: Blocker[];
  warnings?: string[];
  /** Marketplace rows only. */
  installed?: boolean;
  installed_version?: string | null;
  installable?: boolean;
  blockers?: Blocker[];
};

export type PlanStep = {
  id: string;
  name: string;
  version: string;
  action: "install" | "upgrade" | "skip";
  from_version: string | null;
  license: string;
  license_kind: LicenseKind;
  reason: string;
  requested: boolean;
  source: string;
  requires_core: string;
  author: string;
  summary: string;
  dependencies: Dep[];
};

export type Plan = {
  ok: boolean;
  root: string;
  core_version: string;
  steps: PlanStep[];
  blockers: Blocker[];
  warnings: string[];
};

export type Tab = "installed" | "marketplace" | "updates";
export type PanelTab = "overview" | "dependencies" | "contents" | "versions";

/** Where the Modules page is, as the address holds it. */
export type ModulesRoute = { tab: Tab; id: string; panel: PanelTab };
