import { useCallback, useEffect, useRef, useState } from "react";
import { call } from "../api.ts";
import type { Plan, Row } from "./types.ts";

/** Fill in what an older server does not send, so the page can rely on every field. */
export function normalize(r: Partial<Row> & { id: string }): Row {
  return {
    name: r.id,
    version: "",
    description: "",
    summary: "",
    source: "marketplace",
    author: "",
    authors: [],
    license: "",
    license_kind: "none",
    license_url: null,
    homepage: null,
    repository: null,
    documentation: null,
    support: null,
    keywords: [],
    categories: [],
    icon: "",
    maturity: "",
    requires_core: "*",
    compatible: true,
    dependencies: [],
    dependents: [],
    update_available: null,
    models: [],
    groups: [],
    ...r,
  } as Row;
}

export type Loaded = {
  installed: Row[];
  market: Row[];
  loading: boolean;
  error: string;
  reload: () => Promise<void>;
};

/** The installed modules and the marketplace, loaded together and again after any change. */
export function useModules(token: string, enabled = true): Loaded {
  const [installed, setInstalled] = useState<Row[]>([]);
  const [market, setMarket] = useState<Row[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const seq = useRef(0);
  const reload = useCallback(async () => {
    const mine = ++seq.current;
    const [a, b] = await Promise.all([
      call(token, "module.list"),
      call(token, "marketplace_listing.list"),
    ]);
    if (mine !== seq.current) return;
    if (a.kind === "ok") setInstalled((a.value as Row[]).map(normalize));
    // An unreachable marketplace must not hide what is installed.
    if (b.kind === "ok") setMarket((b.value as Row[]).map(normalize));
    setError(a.kind === "error" ? a.message : b.kind === "error" ? b.message : "");
    setLoading(false);
  }, [token]);
  useEffect(() => {
    if (enabled) void reload();
  }, [reload, enabled]);
  return { installed, market, loading, error, reload };
}

/** What installing a marketplace package or an uploaded manifest would do (read-only). */
export async function preview(
  token: string,
  input: { id: string } | { manifest: string },
): Promise<{ plan: Plan } | { error: string }> {
  const r = await call(token, "module_plan.preview", input);
  if (r.kind === "ok") {
    const p = r.value as Plan;
    return {
      plan: { ...p, steps: p.steps ?? [], blockers: p.blockers ?? [], warnings: p.warnings ?? [] },
    };
  }
  return { error: r.kind === "error" ? r.message : "" };
}
