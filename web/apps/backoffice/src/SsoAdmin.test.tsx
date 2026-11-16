// @vitest-environment jsdom
// Sign in with Microsoft for administrators (D91), mounted for real: the section of Customize >
// Security and the People panel with its dialog, a fake `fetch` that answers like the server, and
// jsdom for the DOM. The tests drive the screens the way a person does and read the requests.
// Every id here is invented.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { PeopleSection } from "./People.tsx";
import type { Notify } from "./SecurityAccount.tsx";
import { MicrosoftSignInSection } from "./SsoAdmin.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const TENANT = "11111111-2222-3333-4444-555555555555";
const OTHER_TENANT = "66666666-7777-8888-9999-000000000000";
const APP = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OID = "0b0b0b0b-0001-4000-8000-000000000001";
const SECRET = "a-test-secret-value-that-must-not-leak";

type Call = { method: string; path: string; body: Record<string, unknown> | undefined };
type Provider = {
  tenant: string;
  client_id: string;
  enabled: boolean;
  secret_set: boolean;
  break_glass: string;
  break_glass_enough: boolean;
};
type User = {
  name: string;
  groups: string[];
  admin: boolean;
  disabled: boolean;
  offboarded: boolean;
  mfa_enabled: boolean;
  must_change_password: boolean;
  sessions: number;
  sso_only: boolean;
  sso_oid: string | null;
};

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn<Notify>>;
let calls: Call[];
let server: {
  provider: Provider | null;
  users: User[];
  /** A sentence makes the command of that name answer 400 with it. */
  refuse: Record<string, string>;
};

const person = (name: string, extra: Partial<User> = {}): User => ({
  name,
  groups: ["base.user"],
  admin: false,
  disabled: false,
  offboarded: false,
  mfa_enabled: false,
  must_change_password: false,
  sessions: 0,
  sso_only: false,
  sso_oid: null,
  ...extra,
});
const stored = (extra: Partial<Provider> = {}): Provider => ({
  tenant: TENANT,
  client_id: APP,
  enabled: true,
  secret_set: true,
  break_glass: "root1, root2",
  break_glass_enough: true,
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
      const [path] = String(url).split("?");
      const method = init?.method ?? "GET";
      const body = init?.body ? (JSON.parse(String(init.body)) as Call["body"]) : undefined;
      calls.push({ method, path: path!, body });
      if (path === "/api/auth/users") return json({ users: server.users });
      if (path === "/api/auth/groups") return json({ groups: ["base.user"] });
      const command = /^\/api\/(sso_provider|user_account)\/(\w+)$/.exec(path!);
      if (!command) return json({}, 404);
      const name = `${command[1]}.${command[2]}`;
      if (server.refuse[name]) return json({ error: server.refuse[name] }, 400);
      if (name === "sso_provider.list")
        return json({ value: server.provider ? [server.provider] : [] });
      if (name === "sso_provider.configure") {
        const b = body as {
          tenant: string;
          client_id: string;
          client_secret?: string;
          enabled?: boolean;
        };
        server.provider = {
          ...(server.provider ?? stored({ enabled: true })),
          tenant: b.tenant,
          client_id: b.client_id,
          secret_set: b.client_secret !== undefined || (server.provider?.secret_set ?? false),
          enabled: b.enabled ?? server.provider?.enabled ?? true,
        };
        return json({ value: server.provider });
      }
      if (name === "sso_provider.clear") {
        server.provider = null;
        return json({ value: {} });
      }
      const user = server.users.find((u) => u.name === body?.id);
      if (name === "user_account.bind_sso" && user) user.sso_oid = String(body?.oid);
      if (name === "user_account.unbind_sso" && user) user.sso_oid = null;
      if (name === "user_account.set_sso_only" && user) user.sso_only = body?.sso_only === true;
      return json({ value: user ?? {} });
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
const button = (name: string, within: ParentNode = document.body) =>
  [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => (b.textContent ?? "").trim() === name || b.getAttribute("aria-label") === name,
  );
const click = async (el: Element | null | undefined) => {
  expect(el, "the element to click").toBeTruthy();
  await act(async () => {
    (el as HTMLElement).click();
  });
  await settle();
};
const type = (input: HTMLInputElement | null | undefined, value: string) => {
  expect(input, "the field to fill").toBeTruthy();
  return act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input!, value);
    input!.dispatchEvent(new Event("input", { bubbles: true }));
  });
};
/** The field of a dialog by its visible label. */
const field = (label: string) => {
  const l = [...document.body.querySelectorAll("label")].find((x) =>
    (x.textContent ?? "").trim().startsWith(label),
  );
  return (l?.control as HTMLInputElement | null) ?? null;
};
const sent = (path: string) => calls.filter((c) => c.path === path);

async function mountSection(refreshKey = 0) {
  await act(async () => {
    root.render(
      <MicrosoftSignInSection token="session-token" notify={notify} refreshKey={refreshKey} />,
    );
  });
  await settle();
}
async function mountPeople() {
  await act(async () => {
    root.render(<PeopleSection token="session-token" notify={notify} />);
  });
  await settle();
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  sessionStorage.clear();
  notify = vi.fn<Notify>();
  calls = [];
  server = { provider: null, users: [person("ada"), person("bea")], refuse: {} };
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

describe("the section Sign in with Microsoft", () => {
  it("not set up: the sentence, a badge and one button, nothing else", async () => {
    await mountSection();
    expect(text()).toContain("Sign in with Microsoft");
    expect(text()).toContain("Not set up");
    expect(text()).toContain(
      "People sign in with their Microsoft work account. It creates no account and gives no permission: bind each person under People.",
    );
    expect([...container.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
      "Set up Microsoft sign-in",
    ]);
    expect(container.querySelector('[role="switch"]')).toBeNull();
  });

  it("set up and on: the summary, the switch, Change and Remove, and who can still sign in", async () => {
    server.provider = stored();
    await mountSection();
    expect(text()).toContain("On");
    expect(text()).toContain(TENANT);
    expect(text()).toContain(APP);
    expect(text()).toContain("Stored");
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(true);
    expect(button("Change", container)).toBeTruthy();
    expect(button("Remove", container)).toBeTruthy();
    expect(text()).toContain(
      "Administrators who can still sign in with a password and a second step: root1, root2.",
    );
    // The secret is not on the screen, and the section asked nothing that could carry it.
    expect(text()).not.toContain(SECRET);
  });

  it("off: the badge says Off; with fewer than two other administrators a warning stands in for the line", async () => {
    server.provider = stored({ enabled: false, break_glass: "root1", break_glass_enough: false });
    await mountSection();
    expect(text()).toContain("Off");
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(false);
    expect(text()).toContain(
      "At least two other administrators with a password and a second step are needed before an account can be made Microsoft only.",
    );
    expect(text()).not.toContain("who can still sign in");
  });

  it("the switch sends configure with the stored tenant and application and the new value, and no secret", async () => {
    server.provider = stored();
    await mountSection();
    await click(container.querySelector('[role="switch"]'));
    const [c] = sent("/api/sso_provider/configure");
    expect(c!.body).toEqual({ tenant: TENANT, client_id: APP, enabled: false });
    expect(notify).toHaveBeenCalledWith("Microsoft sign-in is off.");
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(false);
  });

  it("set up: the form shows the redirect address, refuses what is not a GUID before any request, then saves", async () => {
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    expect(dialog()?.textContent).toContain("Set up Microsoft sign-in");
    expect(field("Redirect address")!.value).toBe(`${location.origin}/api/auth/oidc/callback`);
    expect(field("Redirect address")!.readOnly).toBe(true);
    const secret = field("Client secret")!;
    expect(secret.type).toBe("password");
    expect(secret.autocomplete).toBe("off");

    await type(field("Tenant ID"), "common");
    await type(field("Application (client) ID"), "not-a-guid");
    await click(button("Save", dialog()!));
    expect(dialog()?.textContent).toContain("The tenant ID is a GUID.");
    expect(dialog()?.textContent).toContain("The application ID is a GUID.");
    expect(dialog()?.textContent).toContain("Enter the client secret.");
    expect(field("Tenant ID")!.getAttribute("aria-invalid")).toBe("true");
    expect(sent("/api/sso_provider/configure")).toHaveLength(0);

    await type(field("Tenant ID"), TENANT);
    await type(field("Application (client) ID"), APP);
    await type(secret, SECRET);
    await click(button("Save", dialog()!));
    const [c] = sent("/api/sso_provider/configure");
    expect(c!.body).toEqual({ tenant: TENANT, client_id: APP, client_secret: SECRET });
    expect(dialog()).toBeNull();
    expect(notify).toHaveBeenCalledWith("Microsoft sign-in saved.");
    expect(text()).toContain("On");
    // The secret is gone from the page, from the storage and from the notifications.
    expect(text()).not.toContain(SECRET);
    expect(JSON.stringify(notify.mock.calls)).not.toContain(SECRET);
    expect(localStorage.length + sessionStorage.length).toBe(0);
  });

  it("change: configure is sent without client_secret when the field is left empty", async () => {
    server.provider = stored();
    await mountSection();
    await click(button("Change", container));
    expect(field("Tenant ID")!.value).toBe(TENANT);
    expect(field("Client secret")!.value).toBe("");
    expect(dialog()?.textContent).toContain("Leave empty to keep the stored secret.");
    await type(field("Tenant ID"), OTHER_TENANT);
    await click(button("Save", dialog()!));
    const [c] = sent("/api/sso_provider/configure");
    expect(c!.body).toEqual({ tenant: OTHER_TENANT, client_id: APP });
    expect(c!.body).not.toHaveProperty("client_secret");
    expect(text()).toContain(OTHER_TENANT);
  });

  it("a refusal of the server is shown as it words it, and what was typed stays", async () => {
    server.provider = stored();
    server.refuse["sso_provider.configure"] =
      "1 account(s) are bound to another tenant: unbind them before this tenant is set";
    await mountSection();
    await click(button("Change", container));
    await type(field("Tenant ID"), OTHER_TENANT);
    await click(button("Save", dialog()!));
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "1 account(s) are bound to another tenant: unbind them before this tenant is set",
    );
    expect(field("Tenant ID")!.value).toBe(OTHER_TENANT);
    expect(notify).not.toHaveBeenCalled();
  });

  it("Remove asks first, and clear is sent only after the confirmation", async () => {
    server.provider = stored();
    await mountSection();
    await click(button("Remove", container));
    expect(dialog()?.textContent).toContain("Remove Microsoft sign-in?");
    expect(dialog()?.textContent).toContain(
      "The button leaves the sign-in page. Bindings stay. People who sign in with Microsoft only cannot sign in until it is set up again.",
    );
    expect(sent("/api/sso_provider/clear")).toHaveLength(0);
    await click(button("Cancel", dialog()!));
    expect(sent("/api/sso_provider/clear")).toHaveLength(0);

    await click(button("Remove", container));
    await click(button("Remove", dialog()!));
    expect(sent("/api/sso_provider/clear")).toHaveLength(1);
    expect(notify).toHaveBeenCalledWith("Microsoft sign-in removed.");
    expect(text()).toContain("Not set up");
  });
});

describe("People: Microsoft", () => {
  it("shows no column and no button when nothing is configured or bound", async () => {
    await mountPeople();
    expect(text()).toContain("ada");
    expect(text()).not.toContain("Microsoft");
    expect(button("Microsoft", container)).toBeUndefined();
  });

  it("shows the column once a provider is set up, and the badges of a binding", async () => {
    server.provider = stored();
    server.users = [
      person("ada"),
      person("bea", { sso_oid: OID }),
      person("cy", { sso_oid: OID.replace("0b0b", "0c0c"), sso_only: true }),
    ];
    await mountPeople();
    const rows = Object.fromEntries(
      [...container.querySelectorAll("tr[data-person]")].map((r) => [
        r.getAttribute("data-person"),
        r.querySelector('td[data-label="Microsoft"]')?.textContent,
      ]),
    );
    expect(rows).toEqual({ ada: "Not bound", bea: "Bound", cy: "BoundMicrosoft only" });
  });

  it("shows the column with a binding even when the provider was removed", async () => {
    server.users = [person("ada"), person("bea", { sso_oid: OID })];
    await mountPeople();
    expect(container.querySelector("th")).toBeTruthy();
    expect(text()).toContain("Microsoft");
  });

  it("not bound: Bind asks for a GUID, refuses anything else before a request, then binds", async () => {
    server.provider = stored();
    await mountPeople();
    const row = container.querySelector('tr[data-person="ada"]')!;
    await click(button("Microsoft", row));
    expect(dialog()?.textContent).toContain("Microsoft sign-in for ada");
    expect(dialog()?.textContent).toContain(
      "The person's Object ID in the Microsoft Entra admin centre (Users, the person, Object ID). A GUID, not an e-mail address.",
    );
    await type(field("Object ID"), "ada@example.test");
    await click(button("Bind", dialog()!));
    expect(dialog()?.textContent).toContain("The object ID is a GUID, not an e-mail address.");
    expect(sent("/api/user_account/bind_sso")).toHaveLength(0);

    await type(field("Object ID"), ` ${OID} `);
    await click(button("Bind", dialog()!));
    expect(sent("/api/user_account/bind_sso")[0]!.body).toEqual({ id: "ada", oid: OID });
    expect(notify).toHaveBeenCalledWith("ada is bound to Microsoft.");
    expect(dialog()).toBeNull();
    // The table reloaded.
    expect(
      container.querySelector('tr[data-person="ada"] td[data-label="Microsoft"]')?.textContent,
    ).toBe("Bound");
  });

  it("no provider and not bound: it says to set up first and offers nothing else", async () => {
    server.users = [person("ada"), person("bea", { sso_oid: OID })];
    await mountPeople();
    await click(button("Microsoft", container.querySelector('tr[data-person="ada"]')!));
    expect(dialog()?.textContent).toContain("Set up Microsoft sign-in first.");
    expect(field("Object ID")).toBeNull();
    expect(button("Bind", dialog()!)).toBeUndefined();
  });

  it("a server refusal of a binding is shown in the dialog, which stays open", async () => {
    server.provider = stored();
    server.refuse["user_account.bind_sso"] = "that object id is bound to another account already";
    await mountPeople();
    await click(button("Microsoft", container.querySelector('tr[data-person="ada"]')!));
    await type(field("Object ID"), OID);
    await click(button("Bind", dialog()!));
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "that object id is bound to another account already",
    );
    expect(notify).not.toHaveBeenCalled();
  });

  it("bound: shows the object id, Microsoft only sends set_sso_only, and its refusal is shown", async () => {
    server.provider = stored();
    server.users = [person("ada", { sso_oid: OID })];
    server.refuse["user_account.set_sso_only"] =
      "ada has never signed in with Microsoft on this binding: ask them to sign in once with the button first, so that a wrong object id shows now and not as a lockout";
    await mountPeople();
    await click(button("Microsoft", container.querySelector('tr[data-person="ada"]')!));
    expect(dialog()?.textContent).toContain(`Bound to ${OID}`);
    expect(dialog()?.textContent).toContain(
      "The account then refuses its password and its passkeys.",
    );
    await click(field("Microsoft only"));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: true });
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toMatch(/never signed in/);
    expect(field("Microsoft only")!.checked).toBe(false);

    delete server.refuse["user_account.set_sso_only"];
    await click(field("Microsoft only"));
    expect(notify).toHaveBeenCalledWith("ada now signs in with Microsoft only.");
    expect(
      container.querySelector('tr[data-person="ada"] td[data-label="Microsoft"]')?.textContent,
    ).toBe("BoundMicrosoft only");
  });

  it("Microsoft only on: Remove binding waits for it to be turned off, then sends unbind_sso", async () => {
    server.provider = stored();
    server.users = [person("ada", { sso_oid: OID, sso_only: true })];
    await mountPeople();
    await click(button("Microsoft", container.querySelector('tr[data-person="ada"]')!));
    expect(button("Remove binding", dialog()!)!.disabled).toBe(true);
    expect(dialog()?.textContent).toContain("Turn off Microsoft only before removing the binding.");
    await click(field("Microsoft only"));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: false });

    await click(button("Microsoft", container.querySelector('tr[data-person="ada"]')!));
    expect(button("Remove binding", dialog()!)!.disabled).toBe(false);
    await click(button("Remove binding", dialog()!));
    expect(sent("/api/user_account/unbind_sso")[0]!.body).toEqual({ id: "ada" });
    expect(notify).toHaveBeenCalledWith("The Microsoft binding of ada was removed.");
    expect(
      container.querySelector('tr[data-person="ada"] td[data-label="Microsoft"]')?.textContent,
    ).toBe("Not bound");
  });
});
