// @vitest-environment jsdom
// Access by app on People, mounted for real with a fake server that answers like the real one:
// the table (chips, the two row controls), the Access dialog and New person (the picker), and
// Reset two-factor from the More menu. Every name here is invented (example.test).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { PeopleSection } from "./People.tsx";
import { AccessPicker } from "./AccessPicker.tsx";
import { buildAccess, type ModuleLike } from "./access.ts";
import type { Notify } from "./SecurityAccount.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Call = { method: string; path: string; body: Record<string, unknown> | undefined };
type User = {
  name: string;
  groups: string[];
  admin: boolean;
  disabled: boolean;
  offboarded: boolean;
  mfa_enabled: boolean;
  must_change_password: boolean;
  sessions: number;
};

const model = (name: string, read: string, write: string) => ({
  name,
  read_group: read,
  write_group: write,
});
const MODULES = [
  {
    id: "sales",
    name: "Sales",
    groups: ["sales.user", "sales.manager"],
    contents: { models: [model("order", "sales.user", "sales.manager")] },
  },
  {
    id: "crm",
    name: "CRM",
    groups: ["crm.user", "crm.manager", "crm.exporter"],
    contents: {
      models: [model("lead", "crm.user", "crm.manager")],
      groups: ["crm.exporter"],
    },
  },
  {
    id: "notes",
    name: "Notes",
    groups: ["notes.user"],
    contents: { models: [model("note", "notes.user", "notes.user")] },
  },
];
const KNOWN = [
  "platform.admin",
  "api.device_admin",
  "crm.exporter",
  "crm.manager",
  "crm.user",
  "notes.user",
  "sales.manager",
  "sales.user",
];

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn<Notify>>;
let calls: Call[];
let server: {
  users: User[];
  modules: unknown[];
  /** A sentence makes saving groups answer 400 with it. */
  refuseGroups: string;
  /** A status makes the second-factor reset answer with it. */
  mfaStatus: number;
  /** A status makes saving groups and creating a person answer with it and no sentence. */
  failStatus: number;
  /** The module list and the group list both fail. */
  failLists: boolean;
};

const person = (name: string, extra: Partial<User> = {}): User => ({
  name,
  groups: [],
  admin: false,
  disabled: false,
  offboarded: false,
  mfa_enabled: false,
  must_change_password: false,
  sessions: 0,
  ...extra,
});
const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });

function fakeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url).split("?")[0]!;
      const method = init?.method ?? "GET";
      const body = init?.body ? (JSON.parse(String(init.body)) as Call["body"]) : undefined;
      calls.push({ method, path, body });
      if (path === "/api/auth/users" && method === "GET") return json({ users: server.users });
      if (path === "/api/auth/users" && method === "POST")
        return server.failStatus ? json({ error: "no" }, server.failStatus) : json({ user: body?.username });
      if (server.failLists && (path === "/api/auth/groups" || path === "/api/module/list"))
        return json({ error: "down" }, 500);
      if (path === "/api/auth/groups") return json({ groups: KNOWN });
      if (path === "/api/module/list") return json({ value: server.modules });
      const groups = /^\/api\/auth\/users\/([^/]+)\/groups$/.exec(path);
      if (groups) {
        if (server.failStatus) return json({ error: "no" }, server.failStatus);
        if (server.refuseGroups) return json({ error: server.refuseGroups }, 400);
        const u = server.users.find((x) => x.name === decodeURIComponent(groups[1]!));
        if (u) u.groups = body!.groups as string[];
        return json({ user: u?.name });
      }
      if (path === "/api/auth/mfa/reset") {
        if (server.mfaStatus) return json({ error: "no" }, server.mfaStatus);
        const u = server.users.find((x) => x.name === body?.user);
        if (u) u.mfa_enabled = false;
        return json({ reset: true });
      }
      return json({}, 404);
    }),
  );
}

const settle = () =>
  act(async () => {
    await new Promise((done) => setTimeout(done, 0));
  });
const text = () => document.body.textContent ?? "";
const dialog = () =>
  document.body.querySelector<HTMLElement>('[role="dialog"], [role="alertdialog"]');
const click = async (el: Element | null | undefined) => {
  expect(el, "the element to click").toBeTruthy();
  await act(async () => {
    (el as HTMLElement).click();
  });
  await settle();
};
const press = async (el: Element | null | undefined) => {
  expect(el, "the element to press").toBeTruthy();
  (el as HTMLElement).focus();
  await click(el);
};
const key = async (el: Element, k: string) => {
  await act(async () => {
    el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));
  });
  await settle();
};
const row = (name: string) =>
  container.querySelector<HTMLTableRowElement>(`tr[data-person="${name}"]`)!;
const rowButton = (name: string, label: string) =>
  [...row(name).querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
const radios = (app: string, within: ParentNode = document.body) =>
  within.querySelector<HTMLElement>(`[role="radiogroup"][aria-label="Access to ${app}"]`);
const radio = (app: string, label: string) =>
  [...(radios(app)?.querySelectorAll<HTMLButtonElement>('[role="radio"]') ?? [])].find(
    (b) => b.textContent === label,
  );
const checked = (app: string) =>
  [...(radios(app)?.querySelectorAll<HTMLButtonElement>('[role="radio"]') ?? [])]
    .filter((b) => b.getAttribute("aria-checked") === "true")
    .map((b) => b.textContent);
const saved = () =>
  calls.filter((c) => /\/users\/[^/]+\/groups$/.test(c.path)).map((c) => c.body!.groups);
const save = () =>
  [...document.body.querySelectorAll("button")].find((b) => b.textContent === "Save access");

async function mount(me = "root") {
  await act(async () => {
    root.render(<PeopleSection token="t" notify={notify} me={me} />);
  });
  await settle();
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  notify = vi.fn<Notify>();
  calls = [];
  server = {
    users: [
      person("root", { groups: ["platform.admin"], admin: true, mfa_enabled: true }),
      person("ada@example.test", {
        groups: ["sales.user", "sales.manager", "crm.user", "api.device_admin", "gone.role"],
        mfa_enabled: true,
      }),
      person("bea@example.test", { groups: [] }),
      person("cy@example.test", { groups: ["sales.manager"] }),
      person("dee@example.test", {
        groups: ["sales.user", "crm.user", "notes.user", "api.device_admin"],
      }),
    ],
    modules: MODULES,
    refuseGroups: "",
    mfaStatus: 0,
    failStatus: 0,
    failLists: false,
  };
  fakeServer();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
});

describe("the People table", () => {
  it("has one Access column in place of Role and Groups, and the chips say what a person may use", async () => {
    await mount();
    const heads = [...container.querySelectorAll("th")].map((h) => h.textContent);
    expect(heads).toContain("Access");
    expect(heads).not.toContain("Role");
    expect(heads).not.toContain("Groups");
    expect(row("root").querySelector(".access-cell")!.textContent).toBe("Everything");
    expect(row("ada@example.test").querySelector(".access-cell")!.textContent).toContain(
      "Sales · Manager",
    );
    expect(row("ada@example.test").querySelector(".access-cell")!.textContent).toContain(
      "CRM · User",
    );
    expect(row("bea@example.test").querySelector(".access-cell")!.textContent).toBe("No apps yet");
  });

  it("shows Custom for a group that fits no rung, never rewriting it", async () => {
    await mount();
    expect(row("cy@example.test").querySelector(".access-cell")!.textContent).toContain(
      "Sales · Custom",
    );
  });

  it("shows three apps at most, then +N more that opens the dialog", async () => {
    await mount();
    const cell = row("dee@example.test").querySelector(".access-cell")!;
    expect(cell.textContent).toContain("Sales · User");
    expect(cell.textContent).toContain("CRM · User");
    expect(cell.textContent).toContain("Notes · User");
    // Three apps and one group of no app: the fourth item is folded.
    expect(cell.textContent).toContain("+1 more");
    // The chip names the person, so the same chip on two rows is not the same control.
    const chip = cell.querySelector(".access-more-chip")!;
    expect(chip.getAttribute("aria-label")).toBe("+1 more, Access of dee@example.test");
    await press(chip);
    expect(dialog()?.textContent).toContain("Access of dee@example.test");
  });

  it("has two controls per row: Access and More, and the full identifiers in the cell title", async () => {
    await mount();
    expect(rowButton("ada@example.test", "Access")).toBeTruthy();
    expect(rowButton("ada@example.test", "More")).toBeTruthy();
    for (const old of ["Groups", "Disable", "Reset password", "Offboard"])
      expect(rowButton("ada@example.test", old), old).toBeUndefined();
    expect(row("ada@example.test").querySelector(".access-cell")!.getAttribute("title")).toContain(
      "sales.manager",
    );
  });

  it("More holds Reset password, Reset two-factor, Disable and Offboard; arrow keys move and Escape returns focus", async () => {
    await mount();
    const more = rowButton("ada@example.test", "More")!;
    await press(more);
    const items = [...document.body.querySelectorAll('[role="menuitem"]')];
    expect(items.map((i) => i.textContent)).toEqual([
      "Reset password",
      "Reset two-factor",
      "Disable",
      "Offboard",
    ]);
    expect(document.activeElement).toBe(items[0]);
    await key(items[0]!, "ArrowDown");
    expect(document.activeElement).toBe(items[1]);
    await key(items[1]!, "Escape");
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(more);
  });
});

describe("Access of NAME", () => {
  it("changes one app and saves the WHOLE list: every group the picker does not show stays", async () => {
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    expect(dialog()?.textContent).toContain("Access of ada@example.test");
    expect(checked("Sales")).toEqual(["Manager"]);
    expect(checked("CRM")).toEqual(["User"]);
    expect(save()!.disabled).toBe(true);
    await click(radio("CRM", "Manager")!);
    await click(save()!);
    expect(saved()).toHaveLength(1);
    const sent = saved()[0] as string[];
    expect(sent.sort()).toEqual(
      [
        "api.device_admin",
        "crm.manager",
        "crm.user",
        "gone.role",
        "sales.manager",
        "sales.user",
      ].sort(),
    );
    expect(notify).toHaveBeenCalledWith("Access of ada@example.test saved.");
  });

  it("Manager writes both the base group and the manage group", async () => {
    await mount();
    await press(rowButton("bea@example.test", "Access"));
    expect(checked("Sales")).toEqual(["No access"]);
    await click(radio("Sales", "Manager")!);
    await click(save()!);
    expect((saved()[0] as string[]).sort()).toEqual(["sales.manager", "sales.user"]);
  });

  it("an app with one group is a switch", async () => {
    await mount();
    await press(rowButton("bea@example.test", "Access"));
    expect(radios("Notes")).toBeNull();
    const sw = [...document.body.querySelectorAll<HTMLInputElement>('input[role="switch"]')].find(
      (i) => i.closest("label")?.textContent?.startsWith("Can use Notes"),
    )!;
    expect(sw.checked).toBe(false);
    await click(sw);
    await click(save()!);
    expect(saved()[0]).toEqual(["notes.user"]);
  });

  it("does not rewrite a custom state: no choice is selected until one is made", async () => {
    await mount();
    await press(rowButton("cy@example.test", "Access"));
    expect(checked("Sales")).toEqual([]);
    expect(dialog()?.textContent).toContain("Custom: sales.manager only.");
    expect(save()!.disabled).toBe(true);
    // The group stays as stored; the first choice still has a Tab stop.
    expect(radio("Sales", "No access")!.tabIndex).toBe(0);
    await click(radio("Sales", "User")!);
    await click(save()!);
    expect(saved()[0]).toEqual(["sales.user"]);
  });

  it("arrow keys move between the choices of an app and choose", async () => {
    await mount();
    await press(rowButton("bea@example.test", "Access"));
    const none = radio("Sales", "No access")!;
    none.focus();
    await key(none, "ArrowRight");
    expect(checked("Sales")).toEqual(["User"]);
  });

  it("the administrator switch dims the apps and keeps the choices; turning it off restores them", async () => {
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    const admin = document.body.querySelector<HTMLInputElement>(
      'input[role="switch"][aria-describedby]',
    )!;
    expect(admin.checked).toBe(false);
    await click(admin);
    // Said once, by the dimmed rows: no second sentence about it.
    expect(dialog()?.textContent).not.toContain("Not needed");
    expect(radios("Sales")!.closest("fieldset")!.disabled).toBe(true);
    expect(checked("Sales")).toEqual(["Manager"]);
    await click(admin);
    expect(radios("Sales")!.closest("fieldset")!.disabled).toBe(false);
    expect(checked("Sales")).toEqual(["Manager"]);
    // Nothing changed overall: nothing to save.
    expect(save()!.disabled).toBe(true);
  });

  it("sends the old list plus platform.admin when the switch goes on, and the stored list unchanged when it goes off", async () => {
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    const admin = () =>
      document.body.querySelector<HTMLInputElement>('input[role="switch"][aria-describedby]')!;
    await click(admin());
    await click(save()!);
    const stored = ["sales.user", "sales.manager", "crm.user", "api.device_admin", "gone.role"];
    expect(saved()).toHaveLength(1);
    expect((saved()[0] as string[]).sort()).toEqual([...stored, "platform.admin"].sort());
    // Open the same person again (now an administrator) and turn the switch off.
    await press(rowButton("ada@example.test", "Access"));
    expect(admin().checked).toBe(true);
    await click(admin());
    await click(save()!);
    expect(saved()).toHaveLength(2);
    expect((saved()[1] as string[]).sort()).toEqual([...stored].sort());
  });

  it("opens each person with their own groups: A, close, then B saves B's list", async () => {
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    await click(radio("CRM", "Manager")!);
    await click(
      [...document.body.querySelectorAll("button")].find((b) => b.textContent === "Cancel")!,
    );
    await press(rowButton("cy@example.test", "Access"));
    expect(dialog()?.textContent).toContain("Access of cy@example.test");
    expect(save()!.disabled).toBe(true);
    await click(radio("CRM", "User")!);
    await click(save()!);
    expect(saved()).toEqual([["sales.manager", "crm.user"]]);
  });

  it("says the lists could not be loaded, and does not call held groups no longer installed", async () => {
    server.failLists = true;
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    expect(dialog()?.textContent).toContain("Could not load the apps.");
    expect(dialog()?.textContent).not.toContain("No apps to give access to yet.");
    expect(dialog()?.textContent).not.toContain("No longer installed");
  });

  it("on your own row the administrator switch cannot be turned off, and says why", async () => {
    await mount("root");
    await press(rowButton("root", "Access"));
    const admin = document.body.querySelector<HTMLInputElement>(
      'input[role="switch"][aria-describedby]',
    )!;
    expect(admin.checked).toBe(true);
    expect(admin.disabled).toBe(true);
    expect(dialog()?.textContent).toContain(
      "You cannot take administrator access from yourself; ask another administrator.",
    );
  });

  it("another administrator's row can be turned off", async () => {
    server.users.push(person("eve@example.test", { groups: ["platform.admin"], admin: true }));
    await mount("root");
    await press(rowButton("eve@example.test", "Access"));
    const admin = document.body.querySelector<HTMLInputElement>(
      'input[role="switch"][aria-describedby]',
    )!;
    expect(admin.disabled).toBe(false);
  });

  it("shows the server's sentence when it refuses, announces it and moves the focus there", async () => {
    server.refuseGroups = "the last administrator keeps administrator access";
    server.users.push(person("eve@example.test", { groups: ["platform.admin"], admin: true }));
    await mount("root");
    await press(rowButton("eve@example.test", "Access"));
    const admin = document.body.querySelector<HTMLInputElement>(
      'input[role="switch"][aria-describedby]',
    )!;
    await click(admin);
    await click(save()!);
    const alert = dialog()!.querySelector<HTMLElement>('[role="alert"]')!;
    expect(alert.textContent).toBe("the last administrator keeps administrator access");
    expect(document.activeElement).toBe(alert);
  });

  it("after the dialog closes, the focus returns to the Access button that opened it", async () => {
    await mount();
    const open = rowButton("bea@example.test", "Access")!;
    await press(open);
    const cancel = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent === "Cancel",
    )!;
    await click(cancel);
    expect(dialog()).toBeNull();
    expect(document.activeElement).toBe(open);
  });

  it("folds extras and other access, lists a group that is no longer installed, and shows technical names on demand", async () => {
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    const more = [...document.body.querySelectorAll("summary")].find((s) =>
      s.textContent?.startsWith("More access in CRM"),
    )!;
    expect(more.closest("details")!.open).toBe(false);
    expect(dialog()?.textContent).toContain("Exporter");
    const other = [...document.body.querySelectorAll("summary")].find((s) =>
      s.textContent?.startsWith("Other access"),
    )!;
    expect(other.textContent).toBe("Other access (2)");
    expect(dialog()?.textContent).toContain("No longer installed: gone.role");
    expect(dialog()?.textContent).toContain("Api: Device admin");
    expect(dialog()?.querySelector("code")).toBeNull();
    const tech = [...document.body.querySelectorAll<HTMLInputElement>('input[role="switch"]')].find(
      (i) => i.closest("label")?.textContent === "Show technical names",
    )!;
    await click(tech);
    const codes = [...dialog()!.querySelectorAll("code")].map((c) => c.textContent);
    expect(codes).toEqual(
      expect.arrayContaining(["sales.user", "sales.manager", "api.device_admin"]),
    );
    expect(localStorage.getItem("people-technical-names")).toBe("1");
  });

  it("with no module list every group is still listed, under Other access", async () => {
    server.modules = [];
    await mount();
    await press(rowButton("ada@example.test", "Access"));
    expect(radios("Sales")).toBeNull();
    expect(dialog()?.textContent).toContain("Other access (");
    await click(
      [...document.body.querySelectorAll("summary")].find((s) =>
        s.textContent?.startsWith("Other access"),
      )!,
    );
    expect(dialog()?.textContent).toContain("Sales: User");
  });
});

describe("New person", () => {
  it("creates the person with the groups the picker built", async () => {
    await mount();
    await press(
      [...container.querySelectorAll("button")].find((b) => b.textContent === "New person"),
    );
    await click(radio("CRM", "Manager")!);
    const name = dialog()!.querySelector<HTMLInputElement>('input[autocomplete="off"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(
        name,
        "fay@example.test",
      );
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click(
      [...document.body.querySelectorAll("button")].find((b) => b.textContent === "Create person"),
    );
    const created = calls.find((c) => c.method === "POST" && c.path === "/api/auth/users")!;
    expect((created.body!.groups as string[]).sort()).toEqual(["crm.manager", "crm.user"]);
  });
});

describe("Reset two-factor", () => {
  const item = () =>
    [...document.body.querySelectorAll('[role="menuitem"]')].find(
      (i) => i.textContent === "Reset two-factor",
    ) as HTMLElement | undefined;

  it("is offered for a person with two-factor, not for you, not for a person without it", async () => {
    await mount("root");
    await press(rowButton("root", "More"));
    expect(item()).toBeUndefined();
    await key(document.activeElement!, "Escape");
    await press(rowButton("bea@example.test", "More"));
    expect(item()).toBeUndefined();
    await key(document.activeElement!, "Escape");
    await press(rowButton("ada@example.test", "More"));
    expect(item()).toBeTruthy();
  });

  it("says what the person loses, then resets, thanks and refreshes the row", async () => {
    await mount("root");
    await press(rowButton("ada@example.test", "More"));
    await click(item()!);
    const d = dialog()!;
    expect(d.getAttribute("role")).toBe("alertdialog");
    expect(d.textContent).toContain("Reset two-factor for ada@example.test?");
    expect(d.textContent).toContain(
      "ada@example.test is signed out everywhere and loses their authenticator app, passkeys and recovery codes.",
    );
    await click(
      [...d.querySelectorAll("button")].find((b) => b.textContent === "Reset two-factor"),
    );
    const sent = calls.find((c) => c.path === "/api/auth/mfa/reset")!;
    expect(sent.body).toEqual({ user: "ada@example.test" });
    expect(notify).toHaveBeenCalledWith("Two-factor reset for ada@example.test");
    expect(dialog()).toBeNull();
    expect(row("ada@example.test").textContent).toContain("Off");
  });

  it("keeps the dialog open and says why when the server refuses", async () => {
    server.mfaStatus = 400;
    await mount("root");
    await press(rowButton("ada@example.test", "More"));
    await click(item()!);
    await click(
      [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Reset two-factor"),
    );
    const alert = dialog()!.querySelector<HTMLElement>('[role="alert"]')!;
    expect(alert.textContent).toBe("There is no such person, or they have no two-factor to reset.");
    expect(document.activeElement).toBe(alert);
  });
});

describe("the section Reset someone's two-factor", () => {
  it("is gone from Security: nothing in People asks for a typed user name to reset", async () => {
    await mount();
    expect(text()).not.toContain("Reset someone's two-factor");
  });
});

describe("the folds of the picker", () => {
  // The picker mounted on its own, so that the lists and the held groups can arrive AFTER the
  // first render, as they do from `useAccessData` and the person's row.
  const FOLD_MODULES = MODULES.slice(0, 2).map((m) =>
    m.id === "crm"
      ? {
          ...m,
          groups: [...m.groups, "privacy.officer"],
          contents: { ...m.contents, groups: ["crm.exporter", "privacy.officer"] },
        }
      : m,
  ) as unknown as ModuleLike[];
  const FOLD_KNOWN = [...KNOWN, "privacy.officer"];
  const apps = [
    { id: "sales", name: "Sales" },
    { id: "crm", name: "CRM" },
  ];
  const picker = (value: string[], known = FOLD_KNOWN, rows = FOLD_MODULES) => {
    const access = buildAccess(rows, apps, known, value);
    return (
      <AccessPicker access={access} value={value} onChange={() => {}} apps={[]} key="picker" />
    );
  };
  const draw = async (node: React.ReactElement) => {
    await act(async () => root.render(node));
    await settle();
  };
  const fold = (cls: "access-more" | "access-other") =>
    document.body.querySelector<HTMLDetailsElement>(`details.${cls}`)!;
  const tick = async (label: string) => {
    const box = [...container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find(
      (i) => i.closest("label")?.textContent?.startsWith(label),
    );
    await click(box);
  };

  it("More access starts open when the person holds an extra of the app, and closed when none", async () => {
    await draw(picker(["crm.user", "crm.exporter"]));
    expect(fold("access-more").open).toBe(true);
  });

  it("More access starts closed when the person holds no extra", async () => {
    await draw(picker(["crm.user"]));
    expect(fold("access-more").open).toBe(false);
  });

  it("opens when the held groups arrive after the first render, and not before", async () => {
    await draw(picker(["crm.user"]));
    expect(fold("access-more").open).toBe(false);
    await draw(picker(["crm.user", "crm.exporter"]));
    expect(fold("access-more").open).toBe(true);
  });

  it("opens when the module list arrives after the first render", async () => {
    await draw(picker(["crm.user", "crm.exporter"], [], []));
    await draw(picker(["crm.user", "crm.exporter"]));
    expect(fold("access-more").open).toBe(true);
  });

  it("stays open when its last box is unticked, and the hand can still close and open it", async () => {
    let value = ["crm.user", "crm.exporter"];
    const again = () =>
      draw(
        <AccessPicker
          access={buildAccess(FOLD_MODULES, apps, FOLD_KNOWN, value)}
          value={value}
          onChange={(next) => {
            value = next;
          }}
        />,
      );
    await again();
    await tick("Exporter");
    await again();
    expect(value).toEqual(["crm.user"]);
    expect(fold("access-more").open).toBe(true);
    const more = fold("access-more");
    await act(async () => {
      more.open = false;
      more.dispatchEvent(new Event("toggle"));
    });
    await settle();
    expect(fold("access-more").open).toBe(false);
    await act(async () => {
      more.open = true;
      more.dispatchEvent(new Event("toggle"));
    });
    await settle();
    expect(fold("access-more").open).toBe(true);
  });

  it("a fold the hand closed stays closed after a filter removes its row and a cleared filter brings it back", async () => {
    // More than eight apps: the picker offers a filter, and a filtered-out row unmounts.
    const many = [
      ...FOLD_MODULES.slice(1),
      ...Array.from({ length: 8 }, (_, i) => ({
        id: `filler${i}`,
        name: `Filler ${i}`,
        groups: [`filler${i}.user`],
        contents: { models: [] },
      })),
    ] as unknown as ModuleLike[];
    const manyApps = [...apps.slice(1), ...many.slice(1).map((m) => ({ id: m.id, name: m.name }))];
    const value = ["crm.user", "crm.exporter"];
    await draw(
      <AccessPicker
        access={buildAccess(many, manyApps, FOLD_KNOWN, value)}
        value={value}
        onChange={() => {}}
      />,
    );
    const more = fold("access-more");
    expect(more.open).toBe(true);
    await act(async () => {
      more.open = false;
      more.dispatchEvent(new Event("toggle"));
    });
    await settle();
    expect(fold("access-more").open).toBe(false);
    const input = container.querySelector<HTMLInputElement>('input[autocomplete="off"]')!;
    const type = async (v: string) => {
      await act(async () => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, v);
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await settle();
    };
    await type("filler");
    expect(document.body.querySelector("details.access-more")).toBeNull();
    await type("");
    expect(fold("access-more").open).toBe(false);
  });

  it("a held extra does not flicker: the open section is rendered open and stays open", async () => {
    await draw(picker(["crm.user", "crm.exporter"]));
    for (let i = 0; i < 3; i++) {
      await settle();
      expect(fold("access-more").open).toBe(true);
    }
  });

  it("Other access starts open for a held group listed there, and closed for none", async () => {
    await draw(picker(["sales.user", "api.device_admin"]));
    expect(fold("access-other").open).toBe(true);
    await draw(picker(["sales.user"]));
    expect(fold("access-other").open).toBe(false);
  });

  it("Other access starts open for a held group that is no longer installed", async () => {
    await draw(picker(["sales.user", "gone.role"]));
    expect(container.textContent).toContain("No longer installed: gone.role");
    expect(fold("access-other").open).toBe(true);
  });

  it("Other access opens when the held groups arrive after the first render", async () => {
    await draw(picker(["sales.user"]));
    expect(fold("access-other").open).toBe(false);
    await draw(picker(["sales.user", "api.device_admin"]));
    expect(fold("access-other").open).toBe(true);
  });

  it("an extra of another prefix keeps its first part; an extra of the app's own prefix is its role word", async () => {
    await draw(picker(["crm.user"]));
    const more = fold("access-more").textContent ?? "";
    expect(more).toContain("Privacy: Officer");
    expect(more).toContain("Exporter");
    expect(more).not.toContain("CRM: Exporter");
  });
});

// A refusal kept on the screen is worded when it is drawn, so a switch of the language words it
// again (docs/ISSUES.md D9).
describe("a refusal kept in a dialog follows a switch of the language (D9)", () => {
  const WAIT = "Too many attempts. Wait a minute and try again.";
  const WAIT_DE = "Zu viele Versuche. Bitte eine Minute warten.";
  const NO_SUCH = "There is no such person, or they have no two-factor to reset.";
  const NO_SUCH_DE = "Diese Person gibt es nicht, oder sie hat keine Zwei-Faktor-Anmeldung.";
  beforeEach(() => provideCatalogue("de", { [WAIT]: WAIT_DE, [NO_SUCH]: NO_SUCH_DE }));
  const alertText = () => dialog()!.querySelector('[role="alert"]')?.textContent;
  const german = () => act(async () => void (await setLanguage("de", false)));

  it("Reset two-factor", async () => {
    server.mfaStatus = 400;
    await mount("root");
    await press(rowButton("ada@example.test", "More"));
    await click(
      [...document.body.querySelectorAll('[role="menuitem"]')].find(
        (i) => i.textContent === "Reset two-factor",
      ),
    );
    await click(
      [...dialog()!.querySelectorAll("button")].find((b) => b.textContent === "Reset two-factor"),
    );
    expect(alertText()).toBe(NO_SUCH);
    await german();
    expect(alertText()).toBe(NO_SUCH_DE);
  });

  it("the Access dialog", async () => {
    server.failStatus = 429;
    await mount("root");
    await press(rowButton("ada@example.test", "Access"));
    await click(
      document.body.querySelector<HTMLInputElement>('input[role="switch"][aria-describedby]'),
    );
    await click(save()!);
    expect(alertText()).toBe(WAIT);
    await german();
    expect(alertText()).toBe(WAIT_DE);
  });

  it("New person", async () => {
    server.failStatus = 429;
    await mount();
    await press(
      [...container.querySelectorAll("button")].find((b) => b.textContent === "New person"),
    );
    const name = dialog()!.querySelector<HTMLInputElement>('input[autocomplete="off"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(
        name,
        "fay@example.test",
      );
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click(
      [...document.body.querySelectorAll("button")].find((b) => b.textContent === "Create person"),
    );
    expect(alertText()).toBe(WAIT);
    await german();
    expect(alertText()).toBe(WAIT_DE);
  });
});
