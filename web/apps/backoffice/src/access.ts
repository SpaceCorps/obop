// Access by app (D69 amendment, PROPOSED): a view over the permission groups of the installed
// modules. Pure: no React, no translation, nothing that talks to the server. The server stays the
// authority and re-validates every group name; this file only chooses which names go in the list.
//
// Groups do not imply each other (docs/MODULES.md): a write group may create, update and delete
// but does not list. So the rung "Manager" holds BOTH the app's base group and its manage group.
// The server replaces a person's whole group list, so every function here carries over every
// group it does not show.

/** The administrator group: every permission, on every app. */
export const ADMIN = "platform.admin";

/** What `module.list` says about a model, as far as access is concerned. */
type ModelLike = { name: string; read_group?: string; write_group?: string };
/** What `module.list` says about a module, as far as access is concerned. */
export type ModuleLike = {
  id: string;
  name: string;
  groups?: string[];
  contents?: {
    models?: ModelLike[];
    groups?: string[];
    approval_steps?: { group?: string }[];
  };
};
/** A launcher app: its id is the id of its module; its name is the one the launcher shows. */
export type AppLike = { id: string; name: string };

/** Where a person stands in one app. `custom` is a state no rung describes. */
export type Level = "none" | "user" | "manager" | "custom";

export type Ladder = {
  /** The app (module) id. */
  id: string;
  name: string;
  /** The group of the first rung: most models of the app are read through it. */
  base: string;
  /** The group of the second rung (Manager), when the app has one; written together with `base`. */
  manage: string | null;
  /** Every other group of the app: field, command and approval groups, a second write group. */
  extras: string[];
  /** Every group the app owns: base, manage and extras. */
  groups: string[];
  /** The models that use a group, as `[module id, model name]`, for the hint under an extra. */
  uses: Record<string, [string, string][]>;
};

export type Access = {
  /** One ladder per launcher app that owns a group, in launcher order. */
  ladders: Ladder[];
  /** Groups no app owns (platform groups, groups of a module without a launcher entry). */
  other: string[];
  /** Held groups that no module and no part of the server knows any more (also in `other`). */
  stale: Set<string>;
};

/** The part of a group before its first dot: the module (or built-in area) it belongs to. */
export function prefixOf(group: string): string {
  const dot = group.indexOf(".");
  return dot < 0 ? "" : group.slice(0, dot);
}

/** The part after the first dot: the role, such as `user`, `manager` or `device_admin`. */
export function roleOf(group: string): string {
  const dot = group.indexOf(".");
  return dot < 0 ? group : group.slice(dot + 1);
}

/** Every group a module names, wherever it names it. */
export function groupsOfModule(m: ModuleLike): string[] {
  const out = new Set<string>(m.groups ?? []);
  for (const g of m.contents?.groups ?? []) out.add(g);
  for (const s of m.contents?.approval_steps ?? []) if (s.group) out.add(s.group);
  for (const mod of m.contents?.models ?? []) {
    if (mod.read_group) out.add(mod.read_group);
    if (mod.write_group) out.add(mod.write_group);
  }
  out.delete("");
  return [...out];
}

const by = (a: string, b: string) => a.localeCompare(b, "en", { numeric: true });

/**
 * Work out the ladder of every app. `apps` is the launcher in its order; `known` is what the server
 * accepts (`GET /api/auth/groups`); `held` adds groups people hold today, so that none is lost.
 *
 * Owner of a group: the app whose id is the group's prefix; else the first launcher app whose
 * module names it; else nobody ("other"). A module that names another module's group (the Swedish
 * chart names the accounting groups) does not get a row of its own for it.
 */
export function buildAccess(
  rows: ModuleLike[],
  apps: AppLike[],
  known: string[] = [],
  held: string[] = [],
): Access {
  const launcher: AppLike[] =
    apps.length > 0 ? apps : rows.map((r) => ({ id: r.id, name: r.name }));
  const appIds = new Set(launcher.map((a) => a.id));
  const rowOf = new Map(rows.map((r) => [r.id, r]));
  const named = new Map<string, Set<string>>();
  const universe = new Set<string>();
  for (const r of rows)
    for (const g of groupsOfModule(r)) {
      universe.add(g);
      if (!named.has(g)) named.set(g, new Set());
      named.get(g)!.add(r.id);
    }
  for (const g of known) if (g) universe.add(g);
  // A held group the server does not list and no module names is no longer registered. With
  // nothing known at all (the lists could not be loaded) nothing is called stale.
  const stale = new Set<string>();
  for (const g of held)
    if (g && g !== ADMIN && !universe.has(g)) {
      if (universe.size > 0) stale.add(g);
      else universe.add(g);
    }
  universe.delete(ADMIN);

  const ownerOf = (g: string): string | null => {
    const p = prefixOf(g);
    if (p && appIds.has(p)) return p;
    const namers = named.get(g);
    if (namers) for (const a of launcher) if (namers.has(a.id)) return a.id;
    return null;
  };

  // How many models are read or written through each group, over every module.
  const reads = new Map<string, number>();
  const writes = new Map<string, number>();
  const uses = new Map<string, [string, string][]>();
  const use = (g: string | undefined, module: string, model: string) => {
    if (!g) return;
    if (!uses.has(g)) uses.set(g, []);
    uses.get(g)!.push([module, model]);
  };
  for (const r of rows)
    for (const m of r.contents?.models ?? []) {
      if (m.read_group) reads.set(m.read_group, (reads.get(m.read_group) ?? 0) + 1);
      if (m.write_group) writes.set(m.write_group, (writes.get(m.write_group) ?? 0) + 1);
      use(m.read_group, r.id, m.name);
      if (m.write_group !== m.read_group) use(m.write_group, r.id, m.name);
    }

  const owned = new Map<string, string[]>();
  const other: string[] = [];
  for (const g of universe) {
    const o = ownerOf(g);
    if (o === null) other.push(g);
    else owned.set(o, [...(owned.get(o) ?? []), g]);
  }

  const ladders: Ladder[] = [];
  for (const app of launcher) {
    const groups = (owned.get(app.id) ?? []).sort(by);
    if (groups.length === 0 || !rowOf.has(app.id)) {
      // An app without a module row (the Customize app) has nothing to give access to; a module
      // whose groups all belong to others has no ladder either. Groups still count as owned, so
      // they must not vanish: they join "other".
      if (groups.length > 0) other.push(...groups);
      continue;
    }
    const count = (m: Map<string, number>, g: string) => m.get(g) ?? 0;
    const manager = `${app.id}.manager`;
    const rank = (g: string, m: Map<string, number>) => [-count(m, g), g === manager ? 0 : 1, g];
    const cmp = (m: Map<string, number>) => (a: string, b: string) => {
      const x = rank(a, m);
      const y = rank(b, m);
      return (
        (x[0] as number) - (y[0] as number) ||
        (x[1] as number) - (y[1] as number) ||
        by(String(x[2]), String(y[2]))
      );
    };
    let base = groups.filter((g) => count(reads, g) > 0).sort(cmp(reads))[0];
    base ??= groups.includes(`${app.id}.user`)
      ? `${app.id}.user`
      : (groups.find((g) => g !== manager) ?? groups[0]!);
    let manage: string | null =
      groups.filter((g) => g !== base && count(writes, g) > 0).sort(cmp(writes))[0] ?? null;
    if (manage === null && groups.includes(manager) && manager !== base) manage = manager;
    const extras = groups.filter((g) => g !== base && g !== manage);
    const used: Record<string, [string, string][]> = {};
    for (const g of groups) if (uses.has(g)) used[g] = uses.get(g)!;
    ladders.push({ id: app.id, name: app.name, base, manage, extras, groups, uses: used });
  }

  return { ladders, other: [...new Set([...other, ...stale])].sort(by), stale };
}

/** Where `groups` stand in one app. */
export function levelOf(groups: string[], ladder: Ladder): Level {
  const has = (g: string | null) => g !== null && groups.includes(g);
  const holdsAny = ladder.groups.some((g) => groups.includes(g));
  if (!holdsAny) return "none";
  if (!has(ladder.base)) return "custom";
  if (ladder.manage === null) return "user";
  return has(ladder.manage) ? "manager" : "user";
}

/**
 * The group list after choosing a level in one app. Every group outside the app is carried over
 * unchanged: other apps, platform groups, groups no longer registered, the administrator group.
 * Extras are touched only when the level is `none` (all of the app goes) and by `withGroup`.
 */
export function withLevel(groups: string[], ladder: Ladder, level: Level): string[] {
  if (level === "custom") return groups;
  const own = new Set(ladder.groups);
  // Everything outside the app stays; so do the app's extras, unless the person loses the app.
  const out = groups.filter((g) => !own.has(g) || (level !== "none" && ladder.extras.includes(g)));
  if (level === "none") return out;
  const next = [...out];
  if (!next.includes(ladder.base)) next.push(ladder.base);
  if (ladder.manage !== null) {
    const at = next.indexOf(ladder.manage);
    if (level === "manager" && at < 0) next.push(ladder.manage);
    if (level === "user" && at >= 0) next.splice(at, 1);
  }
  return next;
}

/** One group added or removed, everything else as it is. */
export function withGroup(groups: string[], group: string, on: boolean): string[] {
  const rest = groups.filter((g) => g !== group);
  return on ? [...rest, group] : rest;
}

export type Summary = {
  admin: boolean;
  /** The apps the person has any access to, in launcher order. */
  apps: { ladder: Ladder; level: Exclude<Level, "none"> }[];
  /** Held groups that belong to no app (the administrator group not counted). */
  other: string[];
};

/** What a row of the People table says about a person. */
export function summarise(groups: string[], access: Access): Summary {
  const apps: Summary["apps"] = [];
  for (const ladder of access.ladders) {
    const level = levelOf(groups, ladder);
    if (level !== "none") apps.push({ ladder, level });
  }
  const owned = new Set(access.ladders.flatMap((l) => l.groups));
  return {
    admin: groups.includes(ADMIN),
    apps,
    other: groups.filter((g) => g !== ADMIN && !owned.has(g)),
  };
}
