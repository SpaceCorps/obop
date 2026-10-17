// What `instance_update.check` and `instance_update.status` return (D63).

export type UpdateState = "idle" | "requested" | "running" | "succeeded" | "failed" | "rolled_back";

/** One installed package judged against a release's core. */
export type ModuleFit = {
  id: string;
  name: string;
  version: string;
  requires_core: string;
  compatible: boolean;
  reasons: string[];
};

export type ReleaseInfo = {
  version: string;
  published_at: number;
  notes: string;
  image: string;
  min_from: string;
  newer: boolean;
  installable: boolean;
  blockers: string[];
  warnings: string[];
  modules: ModuleFit[];
};

export type InstanceUpdate = {
  mode: "managed" | "self_hosted";
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  feed: string;
  feed_error: string | null;
  checked_at: number;
  releases: ReleaseInfo[];
  dropped: string[];
  instructions: string[];
  backup_dir: string | null;
  state: UpdateState;
  kind: "" | "update" | "rollback";
  target_version: string;
  from_version: string;
  phase: string;
  error: string;
  backup: string;
  started_at: number;
  started_by: string;
  finished_at: number;
  can_rollback: boolean;
};
