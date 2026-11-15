// @vitest-environment jsdom
// The sign-in page inside the phone host app. The app cannot run the sign-in with Microsoft or a
// passkey yet, so the page draws neither button there, and where the instance has Microsoft on it
// says so in one sentence. In a browser the page is what it was. The answer of the server to "is
// Microsoft on" is a stand-in; the host's answer is put into the store as the embed code does.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { HostInfo } from "@obop/host-bridge";
import { SignIn } from "../SignIn.tsx";
import { SignInNotes } from "./SignInNotes.tsx";
import { leaveHost, setHostState } from "./state.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SENTENCE =
  "Sign-in with Microsoft is not available in this app yet. To sign in that way, use your browser.";

const info = (signIn: HostInfo["signIn"]): HostInfo => ({
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn,
});

let container: HTMLDivElement;
let root: Root;

/** The server's answer to `GET /api/auth/sso`. */
function microsoftIs(on: boolean) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ enabled: on }), { status: 200 })),
  );
}

/** A browser that knows passkeys, as Chrome and Safari do. */
function withPasskeys() {
  vi.stubGlobal("PublicKeyCredential", function PublicKeyCredential() {});
  Object.defineProperty(navigator, "credentials", { value: {}, configurable: true });
}

const button = (name: string) =>
  [...container.querySelectorAll("button")].find((b) => b.textContent?.trim() === name);
const note = () => container.querySelector('p[role="note"]');

async function showSignIn() {
  await act(async () => root.render(<SignIn onDone={vi.fn()} />));
  // The answer about Microsoft arrives after the first render.
  await act(async () => {});
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  leaveHost();
  vi.unstubAllGlobals();
  delete (navigator as { credentials?: unknown }).credentials;
});

describe("the note by itself", () => {
  const show = (microsoft: boolean) =>
    act(async () => root.render(<SignInNotes microsoft={microsoft} />));

  it("draws nothing in a browser, whatever the instance has on", async () => {
    await show(true);
    expect(container.innerHTML).toBe("");
  });

  it("draws nothing inside the host when the instance has no Microsoft sign-in", async () => {
    setHostState({ embedded: true, info: info({ passkeys: false, microsoft: false }) });
    await show(false);
    expect(container.innerHTML).toBe("");
  });

  it("says the sentence inside a host that does not allow Microsoft, and names no link", async () => {
    setHostState({ embedded: true, info: info({ passkeys: false, microsoft: false }) });
    await show(true);
    expect(note()?.textContent).toBe(SENTENCE);
    expect(container.querySelector("a")).toBeNull();
    expect(container.querySelectorAll("p")).toHaveLength(1);
  });

  it("says it too while the host has not answered, and stops when the host allows it", async () => {
    setHostState({ embedded: true, info: null });
    await show(true);
    expect(note()?.textContent).toBe(SENTENCE);
    await act(async () =>
      setHostState({ embedded: true, info: info({ passkeys: false, microsoft: true }) }),
    );
    expect(note()).toBeNull();
  });
});

describe("the sign-in page of an instance with Microsoft on", () => {
  it("in a browser has the Microsoft button and the passkey button, and no note", async () => {
    microsoftIs(true);
    withPasskeys();
    await showSignIn();
    expect(button("Sign in with Microsoft")).toBeDefined();
    expect(button("Sign in with a passkey")).toBeDefined();
    expect(note()).toBeNull();
  });

  it("inside the host has neither button, and the sentence above the language choice", async () => {
    microsoftIs(true);
    withPasskeys();
    setHostState({ embedded: true, info: info({ passkeys: false, microsoft: false }) });
    await showSignIn();
    expect(button("Sign in with Microsoft")).toBeUndefined();
    expect(button("Sign in with a passkey")).toBeUndefined();
    expect(button("Sign in")).toBeDefined();
    expect(note()?.textContent).toBe(SENTENCE);
    // The note is the last thing of the form before the language choice.
    const form = container.querySelector("form.signin")!;
    const children = [...form.children];
    expect(children.indexOf(note()!)).toBe(children.length - 2);
  });

  it("draws the buttons when the host's answer arrives and allows them", async () => {
    microsoftIs(true);
    withPasskeys();
    setHostState({ embedded: true, info: null });
    await showSignIn();
    expect(button("Sign in with Microsoft")).toBeUndefined();
    expect(button("Sign in with a passkey")).toBeUndefined();
    await act(async () =>
      setHostState({ embedded: true, info: info({ passkeys: true, microsoft: true }) }),
    );
    expect(button("Sign in with Microsoft")).toBeDefined();
    expect(button("Sign in with a passkey")).toBeDefined();
    expect(note()).toBeNull();
  });
});

describe("the sign-in page of an instance without Microsoft", () => {
  it("inside the host has no note and no passkey button, and is otherwise the same form", async () => {
    microsoftIs(false);
    withPasskeys();
    setHostState({ embedded: true, info: info({ passkeys: false, microsoft: false }) });
    await showSignIn();
    expect(note()).toBeNull();
    expect(button("Sign in with a passkey")).toBeUndefined();
    expect(button("Sign in with Microsoft")).toBeUndefined();
    expect(container.querySelector('input[name="username"]')).not.toBeNull();
    expect(container.querySelector('input[name="password"]')).not.toBeNull();
  });
});
