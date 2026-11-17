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
import { MicrosoftSignInSection, redirectAddress } from "./SsoAdmin.tsx";

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
  /** A status makes `sso_provider.list` answer with that error. */
  listStatus: number;
  /** A status makes the people list (`/api/auth/users`) answer with that error. */
  usersStatus: number;
  /** While it is set, the commands named in `gated` wait for it before they answer. */
  gate: Promise<void> | null;
  gated: string[];
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
      if (path === "/api/auth/users")
        return server.usersStatus
          ? json({ error: "the list is not available" }, server.usersStatus)
          : json({ users: server.users });
      if (path === "/api/auth/groups") return json({ groups: ["base.user"] });
      const command = /^\/api\/(sso_provider|user_account)\/(\w+)$/.exec(path!);
      if (!command) return json({}, 404);
      const name = `${command[1]}.${command[2]}`;
      if (server.gate && server.gated.includes(name)) await server.gate;
      if (server.refuse[name]) return json({ error: server.refuse[name] }, 400);
      if (name === "sso_provider.list" && server.listStatus)
        return json({ error: "the provider list is not available" }, server.listStatus);
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
/** A person's click: the keyboard focus is on the button first, as the browser puts it. */
const press = async (el: Element | null | undefined) => {
  expect(el, "the element to press").toBeTruthy();
  (el as HTMLElement).focus();
  await click(el);
};
const alertDialog = () => document.body.querySelector<HTMLElement>('[role="alertdialog"]');
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

async function mountSection(refreshKey = 0, onChanged?: () => void) {
  await act(async () => {
    root.render(
      <MicrosoftSignInSection
        token="session-token"
        notify={notify}
        refreshKey={refreshKey}
        onChanged={onChanged}
      />,
    );
  });
  await settle();
}
async function mountPeople(me?: string, onSignedOut?: () => void) {
  await act(async () => {
    root.render(
      <PeopleSection token="session-token" notify={notify} me={me} onSignedOut={onSignedOut} />,
    );
  });
  await settle();
}
/** The cell "Microsoft" of one person's row is the button (its name has the person and the state). */
const microsoftButton = (name: string) =>
  container.querySelector<HTMLButtonElement>(
    `tr[data-person="${name}"] td[data-label="Microsoft"] button[aria-label^="Microsoft sign-in for ${name}: "]`,
  );

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  sessionStorage.clear();
  notify = vi.fn<Notify>();
  calls = [];
  server = {
    provider: null,
    users: [person("ada"), person("bea")],
    refuse: {},
    listStatus: 0,
    usersStatus: 0,
    gate: null,
    gated: [],
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
    expect(button("Change settings", container)).toBeTruthy();
    expect(button("Remove", container)).toBeTruthy();
    expect(text()).toContain(
      "Administrators who can still sign in with a password and a second step: root1, root2.",
    );
    // The secret is not on the screen, and the section asked nothing that could carry it.
    expect(text()).not.toContain(SECRET);
  });

  it("off: the badge says Off; with fewer than two administrators there is still no warning under the section", async () => {
    server.provider = stored({ enabled: false, break_glass: "root1", break_glass_enough: false });
    await mountSection();
    expect(text()).toContain("Off");
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(false);
    // The rule is said in the person's dialog, where it matters, not as a failure of the set-up.
    expect(text()).not.toContain("At least two other administrators");
    expect(container.querySelector(".notice")).toBeNull();
    expect(text()).toContain(
      "Administrators who can still sign in with a password and a second step: root1.",
    );
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
    // "off" is ignored by browsers; a password manager must not fill the person's own password here.
    expect(secret.autocomplete).toBe("new-password");
    expect(secret.hasAttribute("data-1p-ignore")).toBe(true);
    expect(secret.getAttribute("data-lpignore")).toBe("true");

    await type(field("Tenant ID"), "common");
    await type(field("Application (client) ID"), "not-a-guid");
    await click(button("Save", dialog()!));
    expect(dialog()?.textContent).toContain(
      "This is not a tenant ID: it has 32 letters and digits in five groups.",
    );
    expect(dialog()?.textContent).toContain(
      "This is not an application ID: it has 32 letters and digits in five groups.",
    );
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
    // The first set-up switches the button on for everyone: the toast says what comes next.
    expect(notify).toHaveBeenCalledWith(
      "Microsoft sign-in saved. The button is on: now bind each person under People.",
    );
    expect(text()).toContain("On");
    // The secret is gone from the page, from the storage and from the notifications.
    expect(text()).not.toContain(SECRET);
    expect(JSON.stringify(notify.mock.calls)).not.toContain(SECRET);
    expect(localStorage.length + sessionStorage.length).toBe(0);
  });

  it("change: configure is sent without client_secret when the field is left empty", async () => {
    server.provider = stored();
    await mountSection();
    await click(button("Change settings", container));
    expect(field("Tenant ID")!.value).toBe(TENANT);
    expect(field("Client secret")!.value).toBe("");
    expect(dialog()?.textContent).toContain("Leave empty to keep the stored secret.");
    await type(field("Tenant ID"), OTHER_TENANT);
    await click(button("Save", dialog()!));
    const [c] = sent("/api/sso_provider/configure");
    expect(c!.body).toEqual({ tenant: OTHER_TENANT, client_id: APP });
    expect(c!.body).not.toHaveProperty("client_secret");
    expect(text()).toContain(OTHER_TENANT);
    expect(notify).toHaveBeenCalledWith("Microsoft sign-in saved.");
  });

  it("a refusal of the server is shown as it words it, and what was typed stays", async () => {
    server.provider = stored();
    server.refuse["sso_provider.configure"] =
      "1 account(s) are bound to another tenant: unbind them before this tenant is set";
    await mountSection();
    await click(button("Change settings", container));
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
    expect(container.querySelector('td[data-label="Microsoft"]')).toBeNull();
    expect(button("Microsoft", container)).toBeUndefined();
    expect(button("Microsoft sign-in for ada", container)).toBeUndefined();
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
    expect(rows).toEqual({ ada: "Not bound", bea: "Bound", cy: "Microsoft only" });
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
    await click(microsoftButton("ada"));
    expect(dialog()?.textContent).toContain("Microsoft sign-in for ada");
    expect(dialog()?.textContent).toContain(
      "Copy it from the person's page in the Microsoft Entra admin centre (Users, the person, Object ID). It looks like 00000000-0000-0000-0000-000000000000.",
    );
    await type(field("Object ID"), "ada@example.test");
    await click(button("Bind", dialog()!));
    expect(dialog()?.textContent).toContain("This is an e-mail address, not an object ID.");
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
    await click(microsoftButton("ada"));
    expect(dialog()?.textContent).toContain("Set up Microsoft sign-in first.");
    expect(field("Object ID")).toBeNull();
    expect(button("Bind", dialog()!)).toBeUndefined();
  });

  it("a server refusal of a binding is shown in the dialog, which stays open", async () => {
    server.provider = stored();
    server.refuse["user_account.bind_sso"] = "that object id is bound to another account already";
    await mountPeople();
    await click(microsoftButton("ada"));
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
    await click(microsoftButton("ada"));
    expect(dialog()?.textContent).toContain(`Bound to ${OID}`);
    expect(dialog()?.textContent).toContain(
      "The account then refuses its password and its passkeys.",
    );
    await click(field("Microsoft only"));
    await click(button("Make Microsoft only", alertDialog()!));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: true });
    // The refusal stands by the checkbox, and the question is gone.
    expect(alertDialog()).toBeNull();
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toMatch(/never signed in/);
    expect(field("Microsoft only")!.checked).toBe(false);

    delete server.refuse["user_account.set_sso_only"];
    await click(field("Microsoft only"));
    await click(button("Make Microsoft only", alertDialog()!));
    expect(notify).toHaveBeenCalledWith("ada now signs in with Microsoft only.");
    expect(
      container.querySelector('tr[data-person="ada"] td[data-label="Microsoft"]')?.textContent,
    ).toBe("Microsoft only");
  });

  it("Microsoft only on: Remove binding waits for it to be turned off, then sends unbind_sso", async () => {
    server.provider = stored();
    server.users = [person("ada", { sso_oid: OID, sso_only: true })];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(button("Remove binding", dialog()!)!.disabled).toBe(true);
    expect(dialog()?.textContent).toContain("Turn off Microsoft only before removing the binding.");
    await click(field("Microsoft only"));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: false });
    // Turning it off needs no question, and the toast says the password must be changed.
    expect(notify).toHaveBeenCalledWith(
      "ada can use the password again and must choose a new one at the next sign-in.",
    );

    await click(microsoftButton("ada"));
    expect(button("Remove binding", dialog()!)!.disabled).toBe(false);
    await click(button("Remove binding", dialog()!));
    await click(button("Remove binding", alertDialog()!));
    expect(sent("/api/user_account/unbind_sso")[0]!.body).toEqual({ id: "ada" });
    expect(notify).toHaveBeenCalledWith("The Microsoft binding of ada was removed.");
    expect(
      container.querySelector('tr[data-person="ada"] td[data-label="Microsoft"]')?.textContent,
    ).toBe("Not bound");
  });
});

/** A promise that the test settles by hand: the server answers when `open()` is called. */
function gate() {
  let open!: () => void;
  const promise = new Promise<void>((done) => (open = done));
  return { promise, open };
}
const escape = () =>
  act(async () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
const clickOutside = () =>
  act(async () => {
    document.body
      .querySelector(".overlay")!
      .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
  });
const submit = (form: HTMLFormElement) =>
  act(async () => {
    form.requestSubmit();
  });
const describedBy = (input: HTMLElement) =>
  (input.getAttribute("aria-describedby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id)?.textContent ?? "")
    .join(" | ");

describe("the section: what cannot be loaded, copy, refusals", () => {
  it("a failed list says so with Try again; a server without the model shows nothing at all", async () => {
    server.listStatus = 500;
    await mountSection();
    expect(text()).toContain("Microsoft sign-in could not be loaded. Try again.");
    // No badge, no set-up button: the state is not known.
    expect(container.querySelector('[role="switch"]')).toBeNull();
    expect(button("Set up Microsoft sign-in", container)).toBeUndefined();
    server.listStatus = 0;
    await click(button("Try again", container));
    expect(text()).toContain("Not set up");
    expect(text()).not.toContain("could not be loaded");

    await act(async () => root.unmount());
    root = createRoot(container);
    server.listStatus = 404;
    await mountSection();
    expect(container.textContent).toBe("");
  });

  it("Copy: the status line is in the page before the copy, is filled by it and is cleared again", async () => {
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    const status = dialog()!.querySelector('[role="status"]')!;
    expect(status).toBeTruthy();
    expect(status.textContent).toBe("");
    const write = vi.fn(async () => {});
    vi.stubGlobal("navigator", { clipboard: { writeText: write } });
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const copy = button("Copy redirect address", dialog()!)!;
      expect(copy.textContent).toBe("Copy");
      await act(async () => {
        copy.click();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(write).toHaveBeenCalledWith(`${location.origin}/api/auth/oidc/callback`);
      // The same element that was there before is the one that now has the words.
      expect(dialog()!.querySelector('[role="status"]')).toBe(status);
      expect(status.textContent).toBe("Copied");
      await act(async () => void vi.advanceTimersByTime(4100));
      expect(status.textContent).toBe("");
    } finally {
      vi.useRealTimers();
    }
  });

  it("Copy that the browser refuses says so in words", async () => {
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    // jsdom has no clipboard and no execCommand: the copy fails.
    await click(button("Copy redirect address", dialog()!));
    expect(dialog()!.querySelector('[role="status"]')?.textContent).toBe(
      "Could not copy. Select the address and copy it.",
    );
  });

  it("the redirect address is the one of the page, whichever address that is, and the hint says what to do when it is wrong", async () => {
    expect(redirectAddress("https://office.example.test:8443")).toBe(
      "https://office.example.test:8443/api/auth/oidc/callback",
    );
    expect(redirectAddress("http://127.0.0.1:8377")).toBe(
      "http://127.0.0.1:8377/api/auth/oidc/callback",
    );
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    expect(describedBy(field("Redirect address")!)).toContain(
      "If sign-in later fails with a redirect address mismatch, ask whoever runs the server to set its public address.",
    );
  });

  it("a refusal of the switch is an error notification, and the switch stays as the server has it", async () => {
    server.provider = stored();
    server.refuse["sso_provider.configure"] = "the tenant is not allowed";
    await mountSection();
    await click(container.querySelector('[role="switch"]'));
    // The question comes only when someone signs in with Microsoft only; nobody does here.
    expect(notify).toHaveBeenCalledWith("the tenant is not allowed", "error");
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(true);
  });

  it("a refusal of Remove stays in the question, which stays open, and nothing is announced as done", async () => {
    server.provider = stored();
    server.refuse["sso_provider.clear"] = "accounts are still bound to this tenant";
    await mountSection();
    await click(button("Remove", container));
    await click(button("Remove", alertDialog()!));
    expect(alertDialog()).toBeTruthy();
    expect(alertDialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "accounts are still bound to this tenant",
    );
    expect(notify).not.toHaveBeenCalled();
    expect(text()).toContain("Stored");
  });

  it("Cancel sends nothing, in the form and in the question", async () => {
    server.provider = stored();
    await mountSection();
    await click(button("Change settings", container));
    await type(field("Tenant ID"), OTHER_TENANT);
    await click(button("Cancel", dialog()!));
    await click(button("Remove", container));
    await click(button("Cancel", alertDialog()!));
    expect(calls.filter((c) => c.method === "POST" && c.path !== "/api/sso_provider/list")).toEqual(
      [],
    );
    expect(notify).not.toHaveBeenCalled();
    expect(text()).toContain(TENANT);
  });

  it("every change tells the page around it, so that People follows", async () => {
    const changed = vi.fn();
    await mountSection(0, changed);
    await click(button("Set up Microsoft sign-in"));
    await type(field("Tenant ID"), TENANT);
    await type(field("Application (client) ID"), APP);
    await type(field("Client secret"), SECRET);
    await click(button("Save", dialog()!));
    expect(changed).toHaveBeenCalledTimes(1);
    await click(container.querySelector('[role="switch"]'));
    expect(changed).toHaveBeenCalledTimes(2);
    await click(button("Remove", container));
    await click(button("Remove", alertDialog()!));
    expect(changed).toHaveBeenCalledTimes(3);
  });
});

describe("the section: Microsoft only people, and the switch", () => {
  const only = (name: string, n: number) =>
    person(name, { sso_oid: `0b0b0b0b-0001-4000-8000-00000000000${n}`, sso_only: true });

  it("says how many people sign in with Microsoft only, and puts it into the Remove question", async () => {
    server.provider = stored();
    server.users = [only("ada", 1), only("bea", 2), person("cy")];
    await mountSection();
    expect(text()).toContain("2 people sign in with Microsoft only.");
    await click(button("Remove", container));
    expect(alertDialog()!.textContent).toContain("2 people sign in with Microsoft only.");
  });

  it("says nothing of it when nobody does", async () => {
    server.provider = stored();
    await mountSection();
    expect(text()).not.toContain("sign in with Microsoft only.");
    await click(button("Remove", container));
    expect(alertDialog()!.textContent).not.toContain("sign in with Microsoft only.");
  });

  it("turning the switch off asks first when someone is Microsoft only, and sends nothing until yes", async () => {
    server.provider = stored();
    server.users = [only("ada", 1), person("bea")];
    await mountSection();
    await click(container.querySelector('[role="switch"]'));
    expect(alertDialog()!.textContent).toContain("Turn off Microsoft sign-in?");
    expect(alertDialog()!.textContent).toContain("1 person signs in with Microsoft only.");
    expect(alertDialog()!.textContent).toContain(
      "They cannot sign in while it is off, and their password is refused.",
    );
    expect(sent("/api/sso_provider/configure")).toHaveLength(0);
    expect(container.querySelector<HTMLInputElement>('[role="switch"]')!.checked).toBe(true);

    await click(button("Cancel", alertDialog()!));
    expect(sent("/api/sso_provider/configure")).toHaveLength(0);
    expect(alertDialog()).toBeNull();

    await click(container.querySelector('[role="switch"]'));
    await click(button("Turn off", alertDialog()!));
    expect(sent("/api/sso_provider/configure")[0]!.body).toEqual({
      tenant: TENANT,
      client_id: APP,
      enabled: false,
    });
    expect(alertDialog()).toBeNull();
    expect(notify).toHaveBeenCalledWith("Microsoft sign-in is off.");
  });

  it("asks also when the list of people cannot be read, with the cautious sentence", async () => {
    server.provider = stored();
    server.usersStatus = 500;
    await mountSection();
    await click(container.querySelector('[role="switch"]'));
    expect(alertDialog()!.textContent).toContain(
      "People who sign in with Microsoft only cannot sign in while it is off.",
    );
    expect(sent("/api/sso_provider/configure")).toHaveLength(0);
  });

  it("turning it on needs no question", async () => {
    server.provider = stored({ enabled: false });
    server.users = [only("ada", 1)];
    await mountSection();
    await click(container.querySelector('[role="switch"]'));
    expect(alertDialog()).toBeNull();
    expect(sent("/api/sso_provider/configure")[0]!.body).toMatchObject({ enabled: true });
  });

  it("the change form warns that a wrong value stops every sign-in; the first set-up does not", async () => {
    server.provider = stored();
    await mountSection();
    await click(button("Change settings", container));
    expect(dialog()!.textContent).toContain(
      "A wrong tenant, application ID or secret stops every Microsoft sign-in",
    );
    await click(button("Cancel", dialog()!));
    server.provider = null;
    await mountSection(1);
    await click(button("Set up Microsoft sign-in"));
    expect(dialog()!.textContent).not.toContain("A wrong tenant");
  });
});

describe("the form: keyboard, announcements, close while busy", () => {
  const fill = async () => {
    await type(field("Tenant ID"), TENANT);
    await type(field("Application (client) ID"), APP);
    await type(field("Client secret"), SECRET);
  };

  it("a refused Save puts the keyboard on the first wrong field, and the error is tied to it", async () => {
    await mountSection();
    await press(button("Set up Microsoft sign-in"));
    await type(field("Application (client) ID"), "nope");
    await click(button("Save", dialog()!));
    const tenant = field("Tenant ID")!;
    expect(document.activeElement).toBe(tenant);
    expect(tenant.getAttribute("aria-invalid")).toBe("true");
    expect(describedBy(tenant)).toContain(
      "This is not a tenant ID: it has 32 letters and digits in five groups.",
    );
    expect(describedBy(field("Application (client) ID")!)).toContain(
      "This is not an application ID: it has 32 letters and digits in five groups.",
    );
    expect(describedBy(field("Client secret")!)).toContain("Enter the client secret.");
    // Once the first is right, the next refused Save goes to the next one.
    await type(tenant, TENANT);
    await click(button("Save", dialog()!));
    expect(document.activeElement).toBe(field("Application (client) ID"));
    expect(sent("/api/sso_provider/configure")).toHaveLength(0);
  });

  it("a field is judged when it is left, not only at Save; an empty one is not nagged", async () => {
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    const tenant = field("Tenant ID")!;
    await act(async () => tenant.focus());
    await act(async () => tenant.blur());
    expect(dialog()!.textContent).not.toContain(
      "This is not a tenant ID: it has 32 letters and digits in five groups.",
    );
    await type(tenant, "common");
    await act(async () => tenant.focus());
    await act(async () => tenant.blur());
    expect(dialog()!.textContent).toContain(
      "This is not a tenant ID: it has 32 letters and digits in five groups.",
    );
    expect(tenant.getAttribute("aria-invalid")).toBe("true");
    // The first Save of the others has not been asked for yet.
    expect(dialog()!.textContent).not.toContain("Enter the client secret.");
  });

  it("Enter in a field saves once, and a second Enter while the request is out sends nothing more", async () => {
    const hold = gate();
    server.gate = hold.promise;
    server.gated = ["sso_provider.configure"];
    await mountSection();
    await click(button("Set up Microsoft sign-in"));
    await fill();
    const form = dialog()!.querySelector("form")!;
    await submit(form);
    await submit(form);
    await submit(form);
    expect(sent("/api/sso_provider/configure")).toHaveLength(1);
    await act(async () => hold.open());
    await settle();
    expect(sent("/api/sso_provider/configure")).toHaveLength(1);
    expect(dialog()).toBeNull();
  });

  it("nothing closes the form while its request is out; the refusal then reaches the person", async () => {
    server.provider = stored();
    const hold = gate();
    server.gate = hold.promise;
    server.gated = ["sso_provider.configure"];
    server.refuse["sso_provider.configure"] = "this tenant is not allowed";
    await mountSection();
    await click(button("Change settings", container));
    await type(field("Tenant ID"), OTHER_TENANT);
    await click(button("Save", dialog()!));
    await escape();
    await clickOutside();
    expect(dialog()).toBeTruthy();
    expect(button("Cancel", dialog()!)!.disabled).toBe(true);
    await act(async () => hold.open());
    await settle();
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "this tenant is not allowed",
    );
    // Now it closes.
    await escape();
    expect(dialog()).toBeNull();
  });

  it("the keyboard goes back to the button that opened the form, and after a first set-up and a Remove to the heading", async () => {
    await mountSection();
    const setUp = button("Set up Microsoft sign-in", container)!;
    await press(setUp);
    await click(button("Cancel", dialog()!));
    expect(document.activeElement).toBe(setUp);

    await press(setUp);
    await fill();
    await click(button("Save", dialog()!));
    // The button that opened the form is gone: the heading has the keyboard, not the page.
    const heading = container.querySelector("h3")!;
    expect(heading.textContent).toBe("Sign in with Microsoft");
    expect(document.activeElement).toBe(heading);

    const change = button("Change settings", container)!;
    await press(change);
    await click(button("Cancel", dialog()!));
    expect(document.activeElement).toBe(change);

    const remove = button("Remove", container)!;
    await press(remove);
    await click(button("Cancel", alertDialog()!));
    expect(document.activeElement).toBe(remove);

    await press(remove);
    await click(button("Remove", alertDialog()!));
    expect(text()).toContain("Not set up");
    expect(document.activeElement).toBe(container.querySelector("h3"));
  });
});

describe("People: Microsoft, the questions and the rules", () => {
  const bound = (extra: Partial<User> = {}) => person("ada", { sso_oid: OID, ...extra });

  it("each row's button is named for its person", async () => {
    server.provider = stored();
    await mountPeople();
    expect(microsoftButton("ada")).toBeTruthy();
    expect(microsoftButton("bea")).toBeTruthy();
    expect(microsoftButton("ada")).not.toBe(microsoftButton("bea"));
  });

  it("the cell is the control: its words are the state, its name has the person and the state, and no second button", async () => {
    server.provider = stored();
    server.users = [
      person("ada"),
      person("bea", { sso_oid: OID }),
      person("cy", { sso_oid: OID.replace("0b0b", "0c0c"), sso_only: true }),
    ];
    await mountPeople();
    const cells = Object.fromEntries(
      ["ada", "bea", "cy"].map((n) => [
        n,
        [microsoftButton(n)!.textContent, microsoftButton(n)!.getAttribute("aria-label")],
      ]),
    );
    expect(cells).toEqual({
      ada: ["Not bound", "Microsoft sign-in for ada: not bound"],
      bea: ["Bound", "Microsoft sign-in for bea: bound"],
      cy: ["Microsoft only", "Microsoft sign-in for cy: Microsoft only"],
    });
    // One control, not two: the row's action buttons are Access and More only.
    const row = container.querySelector('tr[data-person="ada"]')!;
    expect([...row.querySelectorAll("td.cell-actions button")].map((b) => b.textContent)).toEqual([
      "Access",
      "More",
    ]);
    await click(microsoftButton("ada"));
    expect(dialog()?.textContent).toContain("Microsoft sign-in for ada");
  });

  it("the keyboard returns to the cell after a bind", async () => {
    server.provider = stored();
    await mountPeople();
    await press(microsoftButton("ada"));
    await type(field("Object ID"), OID);
    await click(button("Bind", dialog()!));
    expect(dialog()).toBeNull();
    expect(document.activeElement).toBe(microsoftButton("ada"));
    expect(microsoftButton("ada")!.textContent).toBe("Bound");
  });

  it("an own unbinding ends in the app's sign-out, and nothing reads the list again", async () => {
    server.provider = stored();
    server.users = [bound(), person("bea", { sso_oid: OID.replace("0b0b", "0c0c") })];
    const out = vi.fn();
    await mountPeople("Ada", out);
    await click(microsoftButton("ada"));
    await click(button("Remove binding", dialog()!));
    expect(alertDialog()!.textContent).toContain(
      "This is your own account: you are signed out at once.",
    );
    const reads = sent("/api/auth/users").length;
    await click(button("Remove binding", alertDialog()!));
    expect(sent("/api/user_account/unbind_sso")).toHaveLength(1);
    expect(out).toHaveBeenCalledTimes(1);
    expect(sent("/api/auth/users")).toHaveLength(reads);
  });

  it("another person's unbinding signs nobody out and reloads the list", async () => {
    server.provider = stored();
    server.users = [bound(), person("bea", { sso_oid: OID.replace("0b0b", "0c0c") })];
    const out = vi.fn();
    await mountPeople("Ada", out);
    const reads = sent("/api/auth/users").length;
    await click(microsoftButton("bea"));
    await click(button("Remove binding", dialog()!));
    await click(button("Remove binding", alertDialog()!));
    expect(sent("/api/user_account/unbind_sso")).toHaveLength(1);
    expect(out).not.toHaveBeenCalled();
    expect(sent("/api/auth/users").length).toBe(reads + 1);
  });

  it("making your own account Microsoft only signs you out; turning it off does not", async () => {
    server.provider = stored();
    server.users = [bound()];
    const out = vi.fn();
    await mountPeople("ada", out);
    await click(microsoftButton("ada"));
    await click(field("Microsoft only"));
    await click(button("Make Microsoft only", alertDialog()!));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: true });
    expect(out).toHaveBeenCalledTimes(1);

    await act(async () => root.unmount());
    root = createRoot(container);
    server.users = [bound({ sso_only: true })];
    const off = vi.fn();
    await mountPeople("ada", off);
    await click(microsoftButton("ada"));
    await click(field("Microsoft only"));
    expect(sent("/api/user_account/set_sso_only")[1]!.body).toEqual({ id: "ada", sso_only: false });
    expect(off).not.toHaveBeenCalled();
  });

  it("a 401 on the list of people is a signed-out session, never an empty table", async () => {
    server.usersStatus = 401;
    const out = vi.fn();
    await mountPeople("ada", out);
    expect(out).toHaveBeenCalledTimes(1);
    expect(container.querySelector("table")).toBeNull();
    // Any other failure keeps the old behaviour: no sign-out.
    await act(async () => root.unmount());
    root = createRoot(container);
    server.usersStatus = 500;
    const other = vi.fn();
    await mountPeople("ada", other);
    expect(other).not.toHaveBeenCalled();
  });

  it("Microsoft only is off limits for a person when fewer than two OTHER administrators keep a password and a second step", async () => {
    // The list names every such administrator, the person themselves included: ada and root1 are
    // two, but for ada only root1 is another one.
    server.provider = stored({ break_glass: "ada, root1" });
    server.users = [bound()];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(true);
    expect(dialog()!.textContent).toContain(
      "At least two other administrators with a password and a second step are needed before an account can be made Microsoft only.",
    );
    await click(button("Close", dialog()!));

    // Two others: allowed.
    await act(async () => root.unmount());
    root = createRoot(container);
    server.provider = stored({ break_glass: "ada, root1, root2" });
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(false);
    expect(dialog()!.textContent).not.toContain("At least two other administrators");
    await click(button("Close", dialog()!));

    // Already on: it can always be turned off.
    await act(async () => root.unmount());
    root = createRoot(container);
    server.provider = stored({ break_glass: "root1" });
    server.users = [bound({ sso_only: true })];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(false);
  });

  it("the disabled Remove binding has its reason tied to it", async () => {
    server.provider = stored();
    server.users = [bound({ sso_only: true })];
    await mountPeople();
    await click(microsoftButton("ada"));
    const remove = button("Remove binding", dialog()!)!;
    expect(remove.disabled).toBe(true);
    expect(describedBy(remove)).toContain("Turn off Microsoft only before removing the binding.");
  });

  it("Microsoft only asks first, says what it ends, and sends nothing before yes", async () => {
    server.provider = stored();
    server.users = [bound(), person("bea")];
    await mountPeople("root1");
    await click(microsoftButton("ada"));
    await click(field("Microsoft only"));
    const ask = alertDialog()!;
    expect(ask.textContent).toContain("Make ada sign in with Microsoft only?");
    expect(ask.textContent).toContain(
      "The password and passkeys of ada are refused from now on. Their sessions and phone sign-ins end.",
    );
    expect(ask.textContent).not.toContain("your own account");
    expect(sent("/api/user_account/set_sso_only")).toHaveLength(0);
    expect(field("Microsoft only")!.checked).toBe(false);
    await click(button("Cancel", ask));
    expect(sent("/api/user_account/set_sso_only")).toHaveLength(0);
    expect(alertDialog()).toBeNull();
    expect(dialog()).toBeTruthy();

    await click(field("Microsoft only"));
    await click(button("Make Microsoft only", alertDialog()!));
    expect(sent("/api/user_account/set_sso_only")[0]!.body).toEqual({ id: "ada", sso_only: true });
  });

  it("on the signed-in administrator's own account the question says they are signed out", async () => {
    server.provider = stored();
    server.users = [bound()];
    await mountPeople("Ada");
    await click(microsoftButton("ada"));
    await click(field("Microsoft only"));
    expect(alertDialog()!.textContent).toContain(
      "This is your own account: you are signed out at once.",
    );
  });

  it("with the provider off Microsoft only cannot be turned on, and says what to do; turning it off stays possible", async () => {
    server.provider = stored({ enabled: false });
    server.users = [bound()];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(true);
    expect(dialog()!.textContent).toContain(
      "Turn Microsoft sign-in on first (Customize, Security).",
    );
    await click(button("Close", dialog()!));

    await act(async () => root.unmount());
    root = createRoot(container);
    server.users = [bound({ sso_only: true })];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(false);
    expect(dialog()!.textContent).not.toContain("Turn Microsoft sign-in on first");
  });

  it("with no provider at all, a bound person's checkbox is off limits too", async () => {
    server.users = [bound()];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(field("Microsoft only")!.disabled).toBe(true);
    expect(dialog()!.textContent).toContain(
      "Turn Microsoft sign-in on first (Customize, Security).",
    );
  });

  it("Remove binding asks first and says what it ends; Cancel sends nothing", async () => {
    server.provider = stored();
    server.users = [bound()];
    await mountPeople("root1");
    await click(microsoftButton("ada"));
    await click(button("Remove binding", dialog()!));
    const ask = alertDialog()!;
    expect(ask.textContent).toContain("Remove the Microsoft binding of ada?");
    expect(ask.textContent).toContain(
      "Their sessions and phone sign-ins end. Agent tokens and connected apps stay valid. Their password still works.",
    );
    expect(sent("/api/user_account/unbind_sso")).toHaveLength(0);
    await click(button("Cancel", ask));
    expect(sent("/api/user_account/unbind_sso")).toHaveLength(0);
    expect(dialog()).toBeTruthy();
  });

  it("a refusal of Remove binding is shown inside its question, which stays open", async () => {
    server.provider = stored();
    server.users = [bound()];
    server.refuse["user_account.unbind_sso"] =
      "ada signs in with Microsoft only: turn that off first";
    await mountPeople();
    await click(microsoftButton("ada"));
    await click(button("Remove binding", dialog()!));
    await click(button("Remove binding", alertDialog()!));
    expect(alertDialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "ada signs in with Microsoft only: turn that off first",
    );
    expect(notify).not.toHaveBeenCalled();
  });

  it("the keyboard goes back to the row's button after Cancel and Close", async () => {
    server.provider = stored();
    server.users = [bound(), person("bea")];
    await mountPeople();
    const ada = microsoftButton("ada")!;
    await press(ada);
    await click(button("Close", dialog()!));
    expect(document.activeElement).toBe(ada);
    const bea = microsoftButton("bea")!;
    await press(bea);
    await click(button("Cancel", dialog()!));
    expect(document.activeElement).toBe(bea);
  });

  it("while the provider is still loading the dialog says so; when it cannot be loaded it says that", async () => {
    server.users = [person("ada"), person("bea", { sso_oid: OID })];
    const hold = gate();
    server.gate = hold.promise;
    server.gated = ["sso_provider.list"];
    await mountPeople();
    await click(microsoftButton("ada"));
    expect(dialog()!.textContent).toContain("Loading…");
    expect(field("Object ID")).toBeNull();
    server.listStatus = 500;
    await act(async () => hold.open());
    await settle();
    expect(dialog()!.textContent).toContain("Microsoft sign-in could not be loaded. Try again.");
    expect(field("Object ID")).toBeNull();
  });

  it("nothing closes the Bind dialog while its request is out, and a double Enter binds once", async () => {
    server.provider = stored();
    const hold = gate();
    server.gate = hold.promise;
    server.gated = ["user_account.bind_sso"];
    server.refuse["user_account.bind_sso"] = "that object id is bound to another account already";
    await mountPeople();
    await click(microsoftButton("ada"));
    await type(field("Object ID"), OID);
    const form = dialog()!.querySelector("form")!;
    await submit(form);
    await submit(form);
    await escape();
    await clickOutside();
    expect(dialog()).toBeTruthy();
    expect(sent("/api/user_account/bind_sso")).toHaveLength(1);
    await act(async () => hold.open());
    await settle();
    expect(dialog()!.querySelector('[role="alert"]')?.textContent).toBe(
      "that object id is bound to another account already",
    );
  });

  it("a wrong object id puts the keyboard on the field and ties the error to it", async () => {
    server.provider = stored();
    await mountPeople();
    await click(microsoftButton("ada"));
    await type(field("Object ID"), "ada@example.test");
    await click(button("Bind", dialog()!));
    const oid = field("Object ID")!;
    expect(document.activeElement).toBe(oid);
    expect(oid.getAttribute("aria-invalid")).toBe("true");
    expect(describedBy(oid)).toContain("This is an e-mail address, not an object ID.");
  });
});
