// The org chart of Employees: the employees as a tree on their manager (`reports_to_id`). Pure,
// so the shape is tested without a browser.
import type { Row } from "../../api.ts";

export type OrgNode = {
  id: string;
  name: string;
  job: string;
  department: string;
  /** The blob id of the photo, or "". */
  photo: string;
  children: OrgNode[];
  /** Everyone below this person, at any depth. */
  below: number;
};

const text = (v: unknown) => (typeof v === "string" ? v : "");

const byName = (a: OrgNode, b: OrgNode) =>
  a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true });

/**
 * The people at the top (no manager, or a manager the reader cannot see) with their reports below
 * them, sorted by name at every level. A loop of managers (A reports to B, B to A) cannot hang the
 * page: whoever of the loop comes first by name is drawn at the top and the loop is cut there.
 * `departmentOf` names a department id.
 */
export function orgTree(rows: readonly Row[], departmentOf: (id: string) => string): OrgNode[] {
  const nodes = new Map<string, OrgNode>();
  for (const r of rows)
    nodes.set(r.id, {
      id: r.id,
      name: text(r.name) || "?",
      job: text(r.job_title),
      department: r.department_id ? departmentOf(String(r.department_id)) : "",
      photo: text(r.photo),
      children: [],
      below: 0,
    });
  const managerOf = new Map<string, string>();
  for (const r of rows) {
    const m = text(r.reports_to_id);
    if (m && m !== r.id && nodes.has(m)) managerOf.set(r.id, m);
  }
  // Cut every loop: walk up from each person; a person met twice on one walk closes a loop, and the
  // first of the loop by name loses their manager.
  const sorted = [...nodes.values()].sort(byName);
  const settled = new Set<string>();
  for (const start of sorted) {
    const path: string[] = [];
    const onPath = new Set<string>();
    let at: string | undefined = start.id;
    while (at && !settled.has(at)) {
      if (onPath.has(at)) {
        const loop = path.slice(path.indexOf(at)).map((id) => nodes.get(id)!);
        managerOf.delete(loop.sort(byName)[0]!.id);
        break;
      }
      onPath.add(at);
      path.push(at);
      at = managerOf.get(at);
    }
    for (const id of path) settled.add(id);
  }
  const roots: OrgNode[] = [];
  for (const n of sorted) {
    const m = managerOf.get(n.id);
    if (m) nodes.get(m)!.children.push(n);
    else roots.push(n);
  }
  const count = (n: OrgNode): number => {
    n.below = n.children.reduce((s, c) => s + 1 + count(c), 0);
    return n.below;
  };
  roots.forEach(count);
  return roots;
}

/** The ids of the people whose reports show when the chart opens: the first `depth` levels. */
export function openToDepth(roots: readonly OrgNode[], depth: number): Set<string> {
  const out = new Set<string>();
  const walk = (ns: readonly OrgNode[], d: number) => {
    if (d >= depth) return;
    for (const n of ns)
      if (n.children.length) {
        out.add(n.id);
        walk(n.children, d + 1);
      }
  };
  walk(roots, 0);
  return out;
}

/** The ids on the way from the top to every person whose name or job contains `q` (any case). */
export function pathsTo(
  roots: readonly OrgNode[],
  q: string,
): { open: Set<string>; hits: Set<string> } {
  const open = new Set<string>();
  const hits = new Set<string>();
  const needle = q.trim().toLocaleLowerCase();
  if (!needle) return { open, hits };
  const walk = (n: OrgNode, up: string[]) => {
    if (`${n.name} ${n.job}`.toLocaleLowerCase().includes(needle)) {
      hits.add(n.id);
      for (const id of up) open.add(id);
    }
    for (const c of n.children) walk(c, [...up, n.id]);
  };
  for (const r of roots) walk(r, []);
  return { open, hits };
}
