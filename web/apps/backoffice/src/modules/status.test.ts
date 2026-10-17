import { describe, expect, it } from "vite-plus/test";
import {
  buildTree,
  categoriesOf,
  coreBadge,
  dependencyLine,
  dependencyStatus,
  filterRows,
  flattenTree,
  graphOf,
  installState,
  licenseLabel,
  prettyReq,
  shortVersion,
  sortRows,
  withUpdates,
  worstStatus,
} from "./status.ts";
import { normalize } from "./data.ts";
import { modulesHref } from "./route.ts";
import { parseModulesRoute } from "./route.ts";
import type { Dep, Row } from "./types.ts";

const dep = (o: Partial<Dep> & { id: string }): Dep => ({
  name: o.id,
  requirement: "*",
  kind: "package",
  installed_version: null,
  satisfied: false,
  available_version: null,
  available_satisfies: false,
  optional: false,
  ...o,
});

const row = (id: string, o: Partial<Row> = {}): Row =>
  normalize({ id, name: id, version: "0.2.0", ...o });

describe("dependency status", () => {
  it("a satisfied package is satisfied and a satisfied built-in is built in", () => {
    expect(dependencyStatus(dep({ id: "a", installed_version: "0.3.0", satisfied: true }))).toBe(
      "satisfied",
    );
    expect(
      dependencyStatus(
        dep({ id: "base", kind: "built-in", installed_version: "0.2.0", satisfied: true }),
      ),
    ).toBe("builtin");
  });

  it("a built-in is checked against the core: when it fails there is no way to fix it by installing", () => {
    expect(
      dependencyStatus(
        dep({
          id: "base",
          kind: "built-in",
          installed_version: "0.2.0",
          satisfied: false,
          available_satisfies: true,
        }),
      ),
    ).toBe("conflict");
  });

  it("an installed package at the wrong version is upgraded when the marketplace has a fit, else a conflict", () => {
    const old = dep({ id: "a", installed_version: "0.1.0", satisfied: false });
    expect(
      dependencyStatus({ ...old, available_version: "0.3.0", available_satisfies: true }),
    ).toBe("will_upgrade");
    expect(
      dependencyStatus({ ...old, available_version: "0.1.5", available_satisfies: false }),
    ).toBe("conflict");
    expect(dependencyStatus(old)).toBe("conflict");
  });

  it("a module that is not installed will be installed when offered at a fitting version, else it is missing", () => {
    const gone = dep({ id: "a", kind: "missing" });
    expect(
      dependencyStatus({ ...gone, available_version: "0.3.0", available_satisfies: true }),
    ).toBe("will_install");
    expect(
      dependencyStatus({ ...gone, available_version: "2.0.0", available_satisfies: false }),
    ).toBe("missing");
    expect(dependencyStatus(gone)).toBe("missing");
  });

  it("the worst status of several dependencies decides the badge", () => {
    const ok = dep({ id: "a", installed_version: "1.0.0", satisfied: true });
    const will = dep({
      id: "b",
      kind: "missing",
      available_version: "1.0.0",
      available_satisfies: true,
    });
    const bad = dep({ id: "c", kind: "missing" });
    expect(worstStatus([ok, will])).toBe("will_install");
    expect(worstStatus([ok, will, bad])).toBe("missing");
    expect(worstStatus([ok])).toBe("satisfied");
    expect(worstStatus([])).toBeNull();
  });
});

describe("core compatibility and licences", () => {
  it("says which core fits, or both versions when it does not", () => {
    expect(coreBadge({ compatible: true, requires_core: "^0.2" }, "0.2.0")).toEqual({
      tone: "success",
      text: "Works with core 0.2",
    });
    expect(coreBadge({ compatible: false, requires_core: ">=0.3" }, "0.2.0")).toEqual({
      tone: "danger",
      text: "Needs core ≥0.3, you run 0.2",
    });
  });

  it("shows requirements and versions the way people read them", () => {
    expect(prettyReq(">=0.2, <=1")).toBe("≥0.2, ≤1");
    expect(prettyReq("^0.2")).toBe("^0.2");
    expect(shortVersion("0.2.0")).toBe("0.2");
    expect(shortVersion("0.2.1")).toBe("0.2.1");
    expect(shortVersion("1.0.0-rc.1")).toBe("1.0.0-rc.1");
  });

  it("names a proprietary or missing licence instead of an identifier", () => {
    expect(licenseLabel({ license: "MIT OR Apache-2.0", license_kind: "spdx" })).toBe(
      "MIT OR Apache-2.0",
    );
    expect(licenseLabel({ license: "proprietary", license_kind: "proprietary" })).toBe(
      "Proprietary",
    );
    expect(licenseLabel({ license: "", license_kind: "none" })).toBe("No licence declared");
  });
});

describe("card text, filtering and sorting", () => {
  const rows = [
    row("timesheets", {
      name: "Timesheets",
      summary: "Time on tasks",
      keywords: ["time-tracking"],
      categories: ["services"],
      dependencies: [dep({ id: "projects" })],
      installed_at: 100,
    }),
    row("projects", {
      name: "Projects",
      categories: ["services", "productivity"],
      dependents: ["timesheets"],
      installed_at: 300,
    }),
    row("hr", { name: "Human Resources", categories: ["people"], installed_at: 200 }),
  ];
  const name = (r: Row) => r.name;

  it("writes the dependency line of a card", () => {
    expect(dependencyLine(rows[0]!, (id) => id.toUpperCase())).toBe("Needs: PROJECTS");
    expect(dependencyLine(rows[1]!, (id) => id.toUpperCase())).toBe("Required by: TIMESHEETS");
    const both = row("x", {
      dependencies: [dep({ id: "base" }), dep({ id: "hr" })],
      dependents: ["y"],
    });
    expect(dependencyLine(both, (id) => id)).toBe("Needs: base, hr · Required by: y");
    expect(dependencyLine(row("none"), (id) => id)).toBe("");
  });

  it("filters by every word, in any field, and by category", () => {
    expect(filterRows(rows, "time track", "", name).map((r) => r.id)).toEqual(["timesheets"]);
    expect(filterRows(rows, "", "productivity", name).map((r) => r.id)).toEqual(["projects"]);
    expect(filterRows(rows, "human", "people", name).map((r) => r.id)).toEqual(["hr"]);
    expect(filterRows(rows, "nothing", "", name)).toEqual([]);
    expect(categoriesOf(rows)).toEqual(["people", "productivity", "services"]);
  });

  it("sorts by name, by when it was installed and by how much others need it", () => {
    expect(sortRows(rows, "name", name).map((r) => r.id)).toEqual(["hr", "projects", "timesheets"]);
    expect(sortRows(rows, "recent", name).map((r) => r.id)).toEqual([
      "projects",
      "hr",
      "timesheets",
    ]);
    expect(sortRows(rows, "needed", name).map((r) => r.id)[0]).toBe("projects");
  });

  it("lists updates only for installed packages that have a newer version", () => {
    const list = [
      row("a", { update_available: "0.3.0" }),
      row("core", { update_available: "0.3.0", source: "builtin" }),
      row("b"),
    ];
    expect(withUpdates(list).map((r) => r.id)).toEqual(["a"]);
  });

  it("decides what the button of a marketplace card does", () => {
    expect(installState(row("a", { installed: false, installable: true }))).toBe("install");
    expect(installState(row("a", { installed: true }))).toBe("installed");
    expect(installState(row("a", { installed: true, update_available: "1.0.0" }))).toBe("update");
    expect(installState(row("a", { installed: false, compatible: false }))).toBe("incompatible");
    expect(installState(row("a", { installed: false, installable: false }))).toBe("blocked");
  });
});

describe("the dependency tree", () => {
  const index = new Map<string, Row>(
    [
      row("c", {
        dependencies: [
          dep({ id: "b", requirement: "^1", installed_version: "1.2.0", satisfied: true }),
        ],
      }),
      row("b", {
        dependencies: [
          dep({
            id: "a",
            requirement: ">=0.2",
            kind: "missing",
            available_version: "0.3.0",
            available_satisfies: true,
          }),
          dep({
            id: "base",
            requirement: "^0.2",
            kind: "built-in",
            installed_version: "0.2.0",
            satisfied: true,
          }),
        ],
      }),
      row("a", {
        dependencies: [
          dep({ id: "c", requirement: "*", installed_version: "1.0.0", satisfied: true }),
        ],
      }),
    ].map((r) => [r.id, r]),
  );

  it("follows dependencies transitively with each one's status", () => {
    const tree = buildTree("c", index);
    expect(tree).toHaveLength(1);
    expect(tree[0]!.id).toBe("b");
    expect(tree[0]!.status).toBe("satisfied");
    expect(tree[0]!.children.map((n) => [n.id, n.status])).toEqual([
      ["a", "will_install"],
      ["base", "builtin"],
    ]);
  });

  it("stops at a module that is already an ancestor and says so", () => {
    const a = buildTree("c", index)[0]!.children[0]!;
    expect(a.children.map((n) => [n.id, n.cycle])).toEqual([["c", true]]);
    expect(a.children[0]!.children).toEqual([]);
  });

  it("lists every module once, dependencies first", () => {
    const flat = flattenTree(buildTree("c", index)).map((n) => n.id);
    expect(flat).toEqual(["c", "a", "base", "b"]);
  });

  it("has nothing under a module that needs nothing or is unknown", () => {
    expect(buildTree("nobody", index)).toEqual([]);
  });
});

describe("the instance overview", () => {
  it("puts every module one column right of the deepest module it needs and keeps unconnected built-ins out", () => {
    const rows = [
      row("base", { source: "builtin" }),
      row("product", {
        source: "builtin",
        dependencies: [dep({ id: "base", kind: "built-in", satisfied: true })],
      }),
      row("platform", { source: "builtin" }),
      row("hr", {
        source: "upload",
        dependencies: [dep({ id: "base", kind: "built-in", satisfied: true })],
      }),
      row("field", {
        source: "marketplace",
        dependencies: [
          dep({ id: "product", satisfied: true }),
          dep({ id: "hr", satisfied: false }),
        ],
      }),
    ];
    const g = graphOf(rows);
    const layer = Object.fromEntries(g.nodes.map((n) => [n.id, n.layer]));
    expect(layer).toEqual({ base: 0, product: 1, hr: 1, field: 2 });
    expect(g.nodes.find((n) => n.id === "platform")).toBeUndefined();
    expect(g.layers).toBe(3);
    expect(g.edges).toHaveLength(4);
    expect(g.edges.find((e) => e.from === "field" && e.to === "hr")!.satisfied).toBe(false);
    // Two modules in a column get different rows.
    expect(new Set(g.nodes.filter((n) => n.layer === 1).map((n) => n.row)).size).toBe(2);
  });
});

describe("the address of the page", () => {
  it("keeps the list, the open module and its panel in the address", () => {
    expect(modulesHref({})).toBe("#/customize/modules?tab=modules");
    expect(modulesHref({ tab: "marketplace", id: "timesheets", panel: "dependencies" })).toBe(
      "#/customize/modules?view=marketplace&id=timesheets&tab=modules&section=dependencies",
    );
    expect(parseModulesRoute({ view: "updates", id: "a", section: "contents" })).toEqual({
      tab: "updates",
      id: "a",
      panel: "contents",
    });
    expect(parseModulesRoute({ view: "bogus", id: "", section: "" })).toEqual({
      tab: "installed",
      id: "",
      panel: "overview",
    });
  });
});
