import { t } from "@obop/ui";
import type { Dep, Row } from "./types.ts";

/** What a dependency chip says. Derived from the facts the server sends, never guessed. */
export type DepStatus =
  | "satisfied"
  | "builtin"
  | "will_install"
  | "will_upgrade"
  | "missing"
  | "conflict";

export function dependencyStatus(d: Dep): DepStatus {
  if (d.kind === "built-in") return d.satisfied ? "builtin" : "conflict";
  if (d.kind === "package") {
    if (d.satisfied) return "satisfied";
    // Installed, but at a version the dependent does not accept: the marketplace may have one.
    return d.available_satisfies ? "will_upgrade" : "conflict";
  }
  return d.available_satisfies ? "will_install" : "missing";
}

export type StatusTone = "success" | "info" | "warning" | "danger" | "neutral";

export const STATUS_TONE: Record<DepStatus, StatusTone> = {
  satisfied: "success",
  builtin: "neutral",
  will_install: "info",
  will_upgrade: "info",
  missing: "danger",
  conflict: "danger",
};

export function statusLabel(s: DepStatus): string {
  switch (s) {
    case "satisfied":
      return t("Satisfied");
    case "builtin":
      return t("Built in");
    case "will_install":
      return t("Will be installed");
    case "will_upgrade":
      return t("Will be upgraded");
    case "missing":
      return t("Missing");
    case "conflict":
      return t("Conflict");
  }
}

/** The one that matters most of several dependencies: a conflict beats a missing module, and so on. */
export function worstStatus(deps: Dep[]): DepStatus | null {
  const order: DepStatus[] = [
    "conflict",
    "missing",
    "will_upgrade",
    "will_install",
    "satisfied",
    "builtin",
  ];
  const have = new Set(deps.map(dependencyStatus));
  return order.find((s) => have.has(s)) ?? null;
}

/** ">=0.3, <1" as people read it: "≥0.3, <1". */
export function prettyReq(req: string): string {
  return req.replace(/>=/g, "≥").replace(/<=/g, "≤");
}

/** "0.2.0" as "0.2"; a pre-release or a patch keeps its full form. */
export function shortVersion(v: string): string {
  return v.replace(/^(\d+\.\d+)\.0$/, "$1");
}

export type CoreBadge = { tone: "success" | "danger"; text: string };

/** The compatibility badge of a card: green when the running core fits, red with both versions when not. */
export function coreBadge(row: Pick<Row, "compatible" | "requires_core">, core: string): CoreBadge {
  return row.compatible
    ? { tone: "success", text: t("Works with core {version}", { version: shortVersion(core) }) }
    : {
        tone: "danger",
        text: t("Needs core {requirement}, you run {version}", {
          requirement: prettyReq(row.requires_core),
          version: shortVersion(core),
        }),
      };
}

/** The tone of the maturity badge. */
export function maturityTone(m: string): "success" | "info" | "warning" | "neutral" {
  return m === "stable"
    ? "success"
    : m === "beta"
      ? "info"
      : m === "experimental"
        ? "warning"
        : "neutral";
}

export function maturityLabel(m: string): string {
  return m === "stable"
    ? t("Stable")
    : m === "beta"
      ? t("Beta")
      : m === "experimental"
        ? t("Experimental")
        : "";
}

export function sourceLabel(s: string): string {
  return s === "builtin" ? t("Built in") : s === "marketplace" ? t("Marketplace") : t("Upload");
}

export function licenseLabel(row: Pick<Row, "license" | "license_kind">): string {
  if (row.license_kind === "proprietary") return t("Proprietary");
  if (row.license_kind === "none" || !row.license) return t("No licence declared");
  return row.license;
}

/** "Needs: Contacts, HR" and "Required by: Timesheets" for the card, empty when there is nothing to say. */
export function dependencySummary(
  row: Pick<Row, "dependencies" | "dependents">,
  nameOf: (id: string) => string,
): { needs: string[]; requiredBy: string[] } {
  return {
    needs: row.dependencies.map((d) => nameOf(d.id)),
    requiredBy: row.dependents.map(nameOf),
  };
}

export function dependencyLine(
  row: Pick<Row, "dependencies" | "dependents">,
  nameOf: (id: string) => string,
): string {
  const { needs, requiredBy } = dependencySummary(row, nameOf);
  const parts: string[] = [];
  if (needs.length) parts.push(t("Needs: {list}", { list: needs.join(", ") }));
  if (requiredBy.length) parts.push(t("Required by: {list}", { list: requiredBy.join(", ") }));
  return parts.join(" · ");
}

export type SortKey = "name" | "recent" | "needed";

export function sortRows(rows: Row[], key: SortKey, nameOf: (r: Row) => string): Row[] {
  const byName = (a: Row, b: Row) => nameOf(a).localeCompare(nameOf(b));
  const out = [...rows];
  if (key === "recent")
    out.sort((a, b) => (b.installed_at ?? 0) - (a.installed_at ?? 0) || byName(a, b));
  else if (key === "needed")
    out.sort((a, b) => b.dependents.length - a.dependents.length || byName(a, b));
  else out.sort(byName);
  return out;
}

export function filterRows(
  rows: Row[],
  q: string,
  category: string,
  nameOf: (r: Row) => string,
): Row[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((r) => {
    if (category && !r.categories.includes(category)) return false;
    if (!words.length) return true;
    const hay = [
      nameOf(r),
      r.name,
      r.id,
      r.summary,
      r.description,
      r.author,
      ...r.keywords,
      ...r.categories,
    ]
      .join(" ")
      .toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export function categoriesOf(rows: Row[]): string[] {
  return [...new Set(rows.flatMap((r) => r.categories))].sort();
}

export function categoryLabel(c: string): string {
  switch (c) {
    case "finance":
      return t("Finance");
    case "sales":
      return t("Sales");
    case "supply-chain":
      return t("Supply chain");
    case "people":
      return t("People");
    case "services":
      return t("Services");
    case "marketing":
      return t("Marketing");
    case "productivity":
      return t("Productivity");
    case "platform":
      return t("Platform");
    default:
      return t("Other");
  }
}

/** One line of the dependency tree. */
export type TreeNode = {
  id: string;
  name: string;
  requirement: string;
  status: DepStatus;
  installedVersion: string | null;
  availableVersion: string | null;
  /** Set when this module already appears above it: the branch stops here. */
  cycle: boolean;
  children: TreeNode[];
};

/**
 * The dependency tree under `rootId`: each dependency with its own, transitively. `index` holds
 * the modules whose dependencies are known (installed ones win over marketplace listings).
 */
export function buildTree(
  rootId: string,
  index: Map<string, Row>,
  nameOf?: (id: string) => string,
  maxDepth = 8,
): TreeNode[] {
  const walk = (id: string, path: string[]): TreeNode[] => {
    const row = index.get(id);
    if (!row || path.length > maxDepth) return [];
    return row.dependencies.map((d) => {
      const cycle = path.includes(d.id);
      return {
        id: d.id,
        name: nameOf?.(d.id) ?? index.get(d.id)?.name ?? d.name,
        requirement: d.requirement,
        status: dependencyStatus(d),
        installedVersion: d.installed_version,
        availableVersion: d.available_version,
        cycle,
        children: cycle ? [] : walk(d.id, [...path, d.id]),
      };
    });
  };
  return walk(rootId, [rootId]);
}

/** Every module in a tree, once, in the order they would be installed (leaves first). */
export function flattenTree(nodes: TreeNode[]): TreeNode[] {
  const seen = new Set<string>();
  const out: TreeNode[] = [];
  const visit = (n: TreeNode) => {
    n.children.forEach(visit);
    if (!seen.has(n.id)) {
      seen.add(n.id);
      out.push(n);
    }
  };
  nodes.forEach(visit);
  return out;
}

export type GraphNode = { id: string; name: string; layer: number; row: number; builtin: boolean };
export type GraphEdge = { from: string; to: string; requirement: string; satisfied: boolean };

/**
 * The instance's dependency graph for the overview: modules that need nothing on the left, every
 * other module one column right of the deepest module it needs. Built-in modules that nothing
 * connects are left out to keep the picture about relationships.
 */
export function graphOf(rows: Row[]): { nodes: GraphNode[]; edges: GraphEdge[]; layers: number } {
  const byId = new Map(rows.map((r) => [r.id, r]));
  const edges: GraphEdge[] = [];
  for (const r of rows)
    for (const d of r.dependencies)
      if (byId.has(d.id))
        edges.push({ from: r.id, to: d.id, requirement: d.requirement, satisfied: d.satisfied });
  const linked = new Set(edges.flatMap((e) => [e.from, e.to]));
  const layerOf = new Map<string, number>();
  const layer = (id: string, seen: string[] = []): number => {
    const known = layerOf.get(id);
    if (known !== undefined) return known;
    const r = byId.get(id);
    if (!r || seen.includes(id)) return 0;
    const deps = r.dependencies.filter((d) => byId.has(d.id));
    const l = deps.length ? 1 + Math.max(...deps.map((d) => layer(d.id, [...seen, id]))) : 0;
    layerOf.set(id, l);
    return l;
  };
  const nodes: GraphNode[] = [];
  const ids = rows.filter((r) => linked.has(r.id) || r.source !== "builtin").map((r) => r.id);
  const top = Math.max(0, ...ids.map((id) => layer(id)));
  // Within a column, sit each module near what it needs (the mean row of its dependencies in the
  // columns before), so lines cross less; the first column is alphabetical.
  const rowOf = new Map<string, number>();
  for (let l = 0; l <= top; l++) {
    const inLayer = ids.filter((id) => layer(id) === l);
    const mean = (id: string) => {
      const deps = (byId.get(id)?.dependencies ?? [])
        .map((d) => rowOf.get(d.id))
        .filter((x) => x !== undefined);
      return deps.length
        ? deps.reduce((a, b) => a! + b!, 0)! / deps.length
        : Number.MAX_SAFE_INTEGER;
    };
    inLayer.sort((a, b) => mean(a) - mean(b) || a.localeCompare(b));
    inLayer.forEach((id, i) => {
      rowOf.set(id, i);
      nodes.push({
        id,
        name: byId.get(id)!.name,
        layer: l,
        row: i,
        builtin: byId.get(id)!.source === "builtin",
      });
    });
  }
  return { nodes, edges, layers: Math.max(0, ...nodes.map((n) => n.layer)) + 1 };
}

/** Rows that have a newer version in the marketplace. */
export function withUpdates(rows: Row[]): Row[] {
  return rows.filter((r) => r.update_available && r.source !== "builtin");
}

/** The version an install of this row would put in place and where it comes from, for button labels. */
export function installState(
  row: Row,
): "installed" | "update" | "install" | "incompatible" | "blocked" {
  if (row.installed) return row.update_available ? "update" : "installed";
  if (!row.compatible) return "incompatible";
  if (row.installable === false) return "blocked";
  return "install";
}
