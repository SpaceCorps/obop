import { describe, expect, it } from "vite-plus/test";
import {
  ADMIN,
  buildAccess,
  groupsOfModule,
  levelOf,
  summarise,
  withGroup,
  withLevel,
  type Access,
  type ModuleLike,
} from "./access.ts";

// Modules shaped like the rows of `module.list`. Every name here is invented except the group
// conventions the bundled modules use (`<id>.user`, `<id>.manager`, a few extra roles).
const model = (name: string, read: string, write: string) => ({
  name,
  read_group: read,
  write_group: write,
});

const accounting: ModuleLike = {
  id: "accounting",
  name: "Accounting",
  groups: ["accounting.user", "accounting.manager", "accounting.approver"],
  contents: {
    models: [
      model("invoice", "accounting.user", "accounting.manager"),
      model("journal", "accounting.user", "accounting.manager"),
      model("tax", "accounting.user", "accounting.user"),
    ],
    groups: ["accounting.approver"],
    approval_steps: [{ group: "accounting.approver" }],
  },
};
// A chart that names the accounting groups: it gets no row of its own for them.
const accountingSe: ModuleLike = {
  id: "accounting_se",
  name: "Accounting (Sweden)",
  groups: ["accounting.user", "accounting.manager"],
  contents: { models: [model("account", "accounting.user", "accounting.manager")] },
};
// The base group is not called "user".
const helpdesk: ModuleLike = {
  id: "helpdesk",
  name: "Helpdesk",
  groups: ["helpdesk.agent", "helpdesk.manager"],
  contents: {
    models: [
      model("ticket", "helpdesk.agent", "helpdesk.agent"),
      model("queue", "helpdesk.agent", "helpdesk.manager"),
    ],
  },
};
// Most models are written by the base group; the manager group appears only in a command.
const pos: ModuleLike = {
  id: "pos",
  name: "Point of sale",
  groups: ["pos.user", "pos.manager"],
  contents: {
    models: [model("sale", "pos.user", "pos.user"), model("till", "pos.user", "pos.user")],
    groups: ["pos.manager"],
  },
};
// One group only: one switch.
const notes: ModuleLike = {
  id: "notes",
  name: "Notes",
  groups: ["notes.user"],
  contents: { models: [model("note", "notes.user", "notes.user")] },
};
// A ladder that is not User and Manager.
const mileage: ModuleLike = {
  id: "mileage",
  name: "Mileage",
  groups: ["mileage.driver", "mileage.reviewer", "mileage.admin"],
  contents: {
    models: [
      model("trip", "mileage.driver", "mileage.driver"),
      model("report", "mileage.driver", "mileage.reviewer"),
    ],
  },
};
// A module with no launcher entry names a group of its own.
const hidden: ModuleLike = { id: "tools", name: "Tools", groups: ["audit.reader"] };

const ROWS = [accounting, accountingSe, helpdesk, pos, notes, mileage, hidden];
const APPS = ["accounting", "helpdesk", "pos", "notes", "mileage"].map((id) => ({
  id,
  name: id.charAt(0).toUpperCase() + id.slice(1),
}));
const KNOWN = [
  ADMIN,
  "api.device_admin",
  "customize.designer",
  ...ROWS.flatMap((r) => groupsOfModule(r)),
];

const access = (held: string[] = []): Access => buildAccess(ROWS, APPS, KNOWN, held);
const ladder = (id: string, a: Access = access()) => {
  const l = a.ladders.find((x) => x.id === id);
  if (!l) throw new Error(`no ladder for ${id}`);
  return l;
};

describe("owner of a group", () => {
  it("is the app whose id is the prefix, also when another module names the group", () => {
    const a = access();
    expect(ladder("accounting", a).groups).toEqual(
      expect.arrayContaining(["accounting.user", "accounting.manager", "accounting.approver"]),
    );
    // accounting_se is not a launcher app here and owns nothing.
    expect(a.ladders.map((l) => l.id)).not.toContain("accounting_se");
  });

  it("falls back to the first launcher app that names the group", () => {
    const a = buildAccess(
      [{ id: "tools", name: "Tools", groups: ["audit.reader"], contents: { models: [] } }],
      [{ id: "tools", name: "Tools" }],
    );
    expect(a.ladders[0]!.groups).toEqual(["audit.reader"]);
    expect(a.other).toEqual([]);
  });

  it("leaves a group nobody owns in other, and never the administrator group", () => {
    const a = access();
    expect(a.other).toEqual(
      expect.arrayContaining(["api.device_admin", "customize.designer", "audit.reader"]),
    );
    expect(a.other).not.toContain(ADMIN);
  });
});

describe("the ladder", () => {
  it("is User and Manager for accounting, with the approver as an extra", () => {
    const l = ladder("accounting");
    expect(l.base).toBe("accounting.user");
    expect(l.manage).toBe("accounting.manager");
    expect(l.extras).toEqual(["accounting.approver"]);
  });

  it("takes the base from the models read, not from the name", () => {
    const l = ladder("helpdesk");
    expect(l.base).toBe("helpdesk.agent");
    expect(l.manage).toBe("helpdesk.manager");
  });

  it("finds the manager group by name when no model is written through it", () => {
    const l = ladder("pos");
    expect(l.base).toBe("pos.user");
    expect(l.manage).toBe("pos.manager");
    expect(l.extras).toEqual([]);
  });

  it("is one switch when the app has one group", () => {
    const l = ladder("notes");
    expect(l.base).toBe("notes.user");
    expect(l.manage).toBeNull();
  });

  it("makes the write group of the models the second rung, whatever it is called", () => {
    const l = ladder("mileage");
    expect(l.base).toBe("mileage.driver");
    expect(l.manage).toBe("mileage.reviewer");
    expect(l.extras).toEqual(["mileage.admin"]);
  });

  it("lists the models that use a group", () => {
    expect(ladder("accounting").uses["accounting.manager"]).toEqual([
      ["accounting", "invoice"],
      ["accounting", "journal"],
      ["accounting_se", "account"],
    ]);
  });
});

describe("levelOf", () => {
  const l = () => ladder("accounting");
  it("says none, user and manager", () => {
    expect(levelOf([], l())).toBe("none");
    expect(levelOf(["sales.user"], l())).toBe("none");
    expect(levelOf(["accounting.user"], l())).toBe("user");
    expect(levelOf(["accounting.user", "accounting.manager"], l())).toBe("manager");
  });
  it("keeps extras out of the level", () => {
    expect(levelOf(["accounting.user", "accounting.approver"], l())).toBe("user");
  });
  it("is honest: a manager group alone, or an extra alone, is custom", () => {
    expect(levelOf(["accounting.manager"], l())).toBe("custom");
    expect(levelOf(["accounting.approver"], l())).toBe("custom");
  });
  it("is user for a one-switch app, custom for a stray extra", () => {
    expect(levelOf(["notes.user"], ladder("notes"))).toBe("user");
  });
});

describe("withLevel", () => {
  const acc = () => ladder("accounting");
  const carried = ["sales.user", "api.device_admin", "gone.role", ADMIN];

  it("Manager writes both the base group and the manage group", () => {
    const next = withLevel(["accounting.user"], acc(), "manager");
    expect(next).toEqual(expect.arrayContaining(["accounting.user", "accounting.manager"]));
    expect(withLevel([], acc(), "manager").sort()).toEqual([
      "accounting.manager",
      "accounting.user",
    ]);
  });

  it("User removes the manage group, keeps the extras and adds the base group", () => {
    const next = withLevel(
      ["accounting.manager", "accounting.approver", "accounting.user"],
      acc(),
      "user",
    );
    expect(next.sort()).toEqual(["accounting.approver", "accounting.user"]);
    expect(withLevel(["accounting.manager"], acc(), "user")).toEqual(["accounting.user"]);
  });

  it("None removes every group of the app, extras included", () => {
    expect(
      withLevel(
        ["accounting.user", "accounting.manager", "accounting.approver", "sales.user"],
        acc(),
        "none",
      ),
    ).toEqual(["sales.user"]);
  });

  it("carries over every group of other apps, other access, stale groups and the administrator", () => {
    for (const level of ["none", "user", "manager"] as const) {
      const next = withLevel([...carried, "accounting.user"], acc(), level);
      for (const g of carried) expect(next, `${g} at ${level}`).toContain(g);
    }
  });

  it("changes nothing for custom", () => {
    expect(withLevel(["accounting.manager"], acc(), "custom")).toEqual(["accounting.manager"]);
  });

  it("is a switch for one-group apps", () => {
    const n = ladder("notes");
    expect(withLevel([], n, "user")).toEqual(["notes.user"]);
    expect(withLevel(["notes.user", "x.y"], n, "none")).toEqual(["x.y"]);
  });

  it("withGroup adds and removes exactly one group", () => {
    expect(withGroup(["a.b"], "c.d", true)).toEqual(["a.b", "c.d"]);
    expect(withGroup(["a.b", "c.d"], "c.d", false)).toEqual(["a.b"]);
    expect(withGroup(["a.b", "c.d"], "c.d", true)).toEqual(["a.b", "c.d"]);
  });
});

describe("stale groups", () => {
  it("lists a held group that nobody registers, and keeps it out of every ladder", () => {
    const a = access(["accounting.retired", "sales.user"]);
    expect(a.stale.has("accounting.retired")).toBe(true);
    expect(a.other).toContain("accounting.retired");
    expect(ladder("accounting", a).groups).not.toContain("accounting.retired");
    // A group the server lists is not stale even when no module names it.
    expect(a.stale.has("api.device_admin")).toBe(false);
  });

  it("does not offer the wildcard marker the server lists among the known groups", () => {
    const a = buildAccess(ROWS, APPS, [...KNOWN, "*"]);
    expect(a.other).not.toContain("*");
  });

  it("calls nothing stale when the server's group list could not be loaded", () => {
    const a = buildAccess(ROWS, APPS, [], ["api.device_admin"]);
    expect(a.stale.size).toBe(0);
    expect(a.other).toContain("api.device_admin");
  });

  it("calls nothing stale while nothing is loaded", () => {
    const a = buildAccess([], [], [], ["accounting.user"]);
    expect(a.stale.size).toBe(0);
    expect(a.other).toEqual(["accounting.user"]);
  });
});

describe("a module that writes through another role", () => {
  // Two models are written by `odd.approver`, one by `odd.manager`: Manager still means the group
  // the module calls its manager, so choosing it gives the person `odd.manager`.
  const odd: ModuleLike = {
    id: "odd",
    name: "Odd",
    groups: ["odd.user", "odd.manager", "odd.approver"],
    contents: {
      models: [
        model("a", "odd.user", "odd.approver"),
        model("b", "odd.user", "odd.approver"),
        model("c", "odd.user", "odd.manager"),
      ],
    },
  };
  it("prefers <id>.manager as the manage rung when anything is written through it", () => {
    const a = buildAccess([odd], [{ id: "odd", name: "Odd" }]);
    const l = a.ladders[0]!;
    expect([l.base, l.manage, l.extras]).toEqual(["odd.user", "odd.manager", ["odd.approver"]]);
    expect(withLevel([], l, "manager").sort()).toEqual(["odd.manager", "odd.user"]);
  });
});

describe("summarise", () => {
  it("says what a row shows", () => {
    const a = access();
    const s = summarise(
      ["accounting.manager", "accounting.user", "pos.user", "api.device_admin"],
      a,
    );
    expect(s.admin).toBe(false);
    expect(s.apps.map((x) => [x.ladder.id, x.level])).toEqual([
      ["accounting", "manager"],
      ["pos", "user"],
    ]);
    expect(s.other).toEqual(["api.device_admin"]);
    expect(summarise([ADMIN], a).admin).toBe(true);
    expect(summarise([], a)).toEqual({ admin: false, apps: [], other: [] });
  });
});
