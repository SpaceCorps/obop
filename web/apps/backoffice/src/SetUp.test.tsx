// @vitest-environment jsdom
// The set-up page of a new instance (D96): the sign-in page shows it instead of its form while no
// account exists, and the normal form on any doubt. Every state and every message, and what is
// sent to the server (nothing, while the form itself can tell what is wrong).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { SignIn } from "./SignIn.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let status: () => Response | Promise<Response>;
let answer: () => Response;
let posts: { url: string; body: Record<string, unknown> }[];

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const OPEN = { open: true, code_active: true, needs_secret_key: false };

beforeEach(async () => {
  posts = [];
  status = () => json(200, OPEN);
  answer = () => json(200, { token: "obop_t", mfa_enrolment_required: true });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === "/api/auth/setup" && init?.method === "POST") {
        posts.push({ url, body: JSON.parse(String(init.body)) });
        return answer();
      }
      if (url === "/api/auth/setup") return status();
      return json(200, { enabled: false });
    }),
  );
  provideCatalogue("en", {});
  await setLanguage("en", false);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

async function show(onDone = vi.fn()) {
  await act(async () => root.render(<SignIn onDone={onDone} />));
  await act(async () => {});
  return onDone;
}

const heading = () => container.querySelector("h2")?.textContent;
const input = (name: string) => container.querySelector<HTMLInputElement>(`[name="${name}"]`)!;
const button = (text: string) =>
  [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(text))!;
const text = () => container.textContent ?? "";

function type(name: string, value: string) {
  const el = input(name);
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  set.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

async function fill(over: Partial<Record<string, string>> = {}) {
  const v = {
    instance_name: " Acme Test ",
    code: "abcde-fghjk-lmnpq-rstuv",
    username: "Root",
    password: "a long enough password 1",
    repeat: "a long enough password 1",
    ...over,
  };
  for (const [k, x] of Object.entries(v)) type(k, x);
  await act(async () => {
    container.querySelector("form")!.requestSubmit();
  });
  await act(async () => {});
}

describe("which page the sign-in screen shows", () => {
  it("shows the set-up page while no account exists", async () => {
    await show();
    expect(heading()).toBe("Set up this instance");
    expect(text()).toContain(
      "Create the first administrator. This page works only until the first account exists.",
    );
    expect(button("Create the administrator")).toBeTruthy();
    expect(button("Sign in")).toBeUndefined();
  });

  for (const [why, s] of [
    ["closed", () => json(200, { open: false })],
    ["an odd answer", () => json(200, { enabled: false })],
    ["a refused answer", () => json(503, { code: "storage_busy" })],
    ["no route (dev-auth)", () => json(404, {})],
    ["no server", () => Promise.reject(new TypeError("offline"))],
  ] as const)
    it(`shows the sign-in form on ${why}`, async () => {
      status = s as () => Response;
      await show();
      expect(heading()).toBe("Sign in");
      expect(text()).not.toContain("Set up this instance");
    });
});

describe("the form", () => {
  it("labels every field, and asks for a new password twice and a plain code", async () => {
    await show();
    for (const label of [
      "Name of this instance (optional)",
      "Set-up code",
      "Username",
      "Password",
      "Repeat the password",
    ])
      expect([...container.querySelectorAll("label")].map((l) => l.textContent)).toContainEqual(
        expect.stringContaining(label),
      );
    expect(input("password").autocomplete).toBe("new-password");
    expect(input("repeat").autocomplete).toBe("new-password");
    expect(input("code").type).toBe("text");
    expect(input("code").getAttribute("spellcheck")).toBe("false");
    expect(input("code").getAttribute("autocapitalize")).toBe("characters");
    expect(input("code").autocomplete).toBe("off");
    expect(input("code").getAttribute("aria-describedby")).toBeTruthy();
    expect(text()).toContain("find the line that starts with “Set-up code”");
  });

  it("says when no code is active and keeps the form", async () => {
    status = () => json(200, { ...OPEN, code_active: false });
    await show();
    expect(text()).toContain(
      "No set-up code is active. Restart the server: it prints a new code when it starts.",
    );
    expect(input("code").disabled).toBe(false);
  });

  it("without a server key says what to do and turns the form off", async () => {
    status = () => json(200, { ...OPEN, code_active: false, needs_secret_key: true });
    await show();
    expect(text()).toContain("Set OBOP_SECRET_KEY in the environment of this service");
    expect(button("Create the administrator").disabled).toBe(true);
    for (const n of ["instance_name", "code", "username", "password", "repeat"])
      expect(input(n).disabled, n).toBe(true);
  });
});

describe("what the form can tell by itself never reaches the server", () => {
  it("two passwords that differ", async () => {
    await show();
    await fill({ repeat: "another long password 2" });
    expect(text()).toContain("The two passwords are not the same.");
    expect(document.activeElement).toBe(input("repeat"));
    expect(input("repeat").getAttribute("aria-invalid")).toBe("true");
    expect(posts).toHaveLength(0);
  });

  it("a short password", async () => {
    await show();
    await fill({ password: "short", repeat: "short" });
    expect(text()).toContain("The password must be at least 12 characters.");
    expect(document.activeElement).toBe(input("password"));
    expect(posts).toHaveLength(0);
  });

  it("a username with other characters, which takes the focus first", async () => {
    await show();
    await fill({ username: "a b", repeat: "different" });
    expect(text()).toContain("The username may only use letters, digits and . _ - @");
    expect(document.activeElement).toBe(input("username"));
    expect(posts).toHaveLength(0);
  });
});

describe("the answers of the server", () => {
  it("creates the administrator and hands the session on as a sign-in does", async () => {
    const onDone = await show();
    await fill();
    expect(posts).toEqual([
      {
        url: "/api/auth/setup",
        body: {
          code: "abcde-fghjk-lmnpq-rstuv",
          username: "Root",
          password: "a long enough password 1",
          instance_name: "Acme Test",
        },
      },
    ]);
    expect(onDone).toHaveBeenCalledWith("obop_t", "root", true);
  });

  it("leaves the name out when none is typed", async () => {
    await show();
    await fill({ instance_name: "  " });
    expect(posts[0]!.body).not.toHaveProperty("instance_name");
  });

  for (const [what, res, sentence] of [
    [
      "a wrong code",
      () => json(403, { code: "setup_code_invalid" }),
      "The set-up code is not right, or it has run out. Check the log of the server, or restart it to print a new code.",
    ],
    [
      "too many attempts",
      () => json(429, { code: "rate_limited" }),
      "Too many attempts. Wait a few minutes and try again.",
    ],
    [
      "a busy database",
      () => json(503, { code: "storage_busy" }),
      "The server is busy. Try again in a moment.",
    ],
    [
      "other input the server refuses",
      () => json(400, { code: "invalid_input", error: "username must be letters" }),
      "Check what you entered and try again.",
    ],
    ["no server", () => Promise.reject(new TypeError("offline")), "Could not reach the server."],
  ] as const)
    it(`${what}: says so, announced and focused, and keeps what was typed`, async () => {
      answer = res as () => Response;
      const onDone = await show();
      await fill();
      const alert = container.querySelector('[role="alert"]');
      expect(alert?.textContent).toContain(sentence);
      expect(document.activeElement).toBe(alert?.parentElement);
      expect(input("username").value).toBe("Root");
      expect(onDone).not.toHaveBeenCalled();
    });

  it("a name the server refuses: says so under the name, in the page's words, and moves there", async () => {
    answer = () =>
      json(400, {
        code: "setup_instance_name_invalid",
        error: "the instance name is refused: name: must not hold < or >, in any field",
      });
    const onDone = await show();
    await fill();
    const sentence =
      "This name of the instance cannot be used. Choose a plain name, or leave the field empty.";
    expect(text()).toContain(sentence);
    expect(text()).not.toContain("must not hold");
    expect(document.activeElement).toBe(input("instance_name"));
    expect(input("instance_name").getAttribute("aria-invalid")).toBe("true");
    expect(input("instance_name").value).toBe(" Acme Test ");
    expect(onDone).not.toHaveBeenCalled();
  });

  it("never shows the server's own English sentence for a refused input", async () => {
    answer = () => json(400, { code: "invalid_input", error: "name: must not hold < or >" });
    await show();
    await fill();
    expect(text()).not.toContain("must not hold");
    expect(text()).toContain("Check what you entered and try again.");
  });

  it("an account made meanwhile: says so and goes to sign in", async () => {
    answer = () => json(409, { code: "setup_closed" });
    await show();
    await fill();
    expect(text()).toContain(
      "This instance already has an administrator. Sign in with that account.",
    );
    await act(async () => button("Go to sign in").click());
    expect(heading()).toBe("Sign in");
  });

  it("no server key after all: says what to do and turns the form off", async () => {
    answer = () => json(503, { code: "setup_needs_secret_key" });
    await show();
    await fill();
    expect(text()).toContain("Set OBOP_SECRET_KEY in the environment of this service");
    expect(button("Create the administrator").disabled).toBe(true);
  });
});
