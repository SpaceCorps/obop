// @vitest-environment jsdom
// Customize > Mail for a holder of mail.admin who may not list the Connectors connections: the tab
// does not ask for them (the server answers 404, a console error on every visit), and it still
// loads the mail servers and the deliveries.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { MailTab } from "./MailTab.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let asked: string[];

beforeEach(() => {
  asked = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const path = new URL(url, "http://localhost").pathname;
      asked.push(path);
      return new Response(JSON.stringify({ value: { rows: [] }, denied: [] }), { status: 200 });
    }),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

async function settle() {
  for (let i = 0; i < 6; i++) await act(async () => await new Promise((r) => setTimeout(r, 0)));
}

it("asks for the connections only of a person who may list them", async () => {
  act(() => root.render(<MailTab token="tok" notify={() => {}} readsConnections={false} />));
  await settle();
  expect(asked).toContain("/api/mail_server/search");
  expect(asked).toContain("/api/outbound_mail/search");
  expect(asked).not.toContain("/api/connection/search");

  act(() => root.unmount());
  root = createRoot(container);
  asked = [];
  act(() => root.render(<MailTab token="tok" notify={() => {}} readsConnections />));
  await settle();
  expect(asked).toContain("/api/connection/search");
});

it("sends the private network switch only when a person turns it on, and warns while it is on", async () => {
  const bodies: Record<string, unknown>[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/mail_server/create") bodies.push(JSON.parse(String(init?.body ?? "{}")));
      return new Response(JSON.stringify({ value: { rows: [], id: "s1" }, denied: [] }), {
        status: 200,
      });
    }),
  );
  act(() => root.render(<MailTab token="tok" notify={() => {}} readsConnections={false} />));
  await settle();
  const open = async () => {
    const add = [...container.querySelectorAll("button")].find(
      (b) => b.textContent === "Add mail server",
    );
    act(() => add!.click());
    await settle();
    const form = container.querySelector("form")!;
    const sw = [...form.querySelectorAll<HTMLInputElement>("input[role=switch]")].find((i) =>
      i.closest("label")?.textContent?.includes("own network"),
    )!;
    return { form, sw };
  };
  const submit = async (form: HTMLFormElement) => {
    await act(async () =>
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })),
    );
    await settle();
  };
  let { form, sw } = await open();
  expect(sw.checked).toBe(false);
  expect(form.textContent).toContain("a relay on a private address is refused");
  act(() => sw.click());
  expect(form.textContent).toContain("a password can travel unencrypted");
  act(() => sw.click());
  await submit(form);
  expect(bodies).toHaveLength(1);
  expect(bodies[0]).not.toHaveProperty("allow_private_network");

  ({ form, sw } = await open());
  act(() => sw.click());
  await submit(form);
  expect(bodies).toHaveLength(2);
  expect(bodies[1]).toHaveProperty("allow_private_network", true);
});
