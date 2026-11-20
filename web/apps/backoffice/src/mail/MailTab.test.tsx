// @vitest-environment jsdom
// Customize > Mail for a holder of mail.admin who may not list the Connectors connections: the tab
// does not ask for them (the server answers 404, a console error on every visit), and it still
// loads the mail servers and the deliveries.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { ModuleMeta } from "../api.ts";
import { registerMeta } from "../serverMessage.ts";
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

it("words the outcome of the last test and keeps the server's answer as the detail", async () => {
  const servers = [
    {
      id: "s1",
      name: "Relay",
      kind: "smtp",
      host: "smtp.example.org",
      last_test_at: "2026-11-20T10:00:00Z",
      last_test_result: "failed: 550 no such user",
      last_test_outcome: "permanent",
    },
    {
      id: "s2",
      name: "Old",
      kind: "smtp",
      host: "smtp.example.org",
      last_test_at: "2026-11-20T10:00:00Z",
      last_test_result: "sent 250 ok",
    },
  ];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const path = new URL(url, "http://localhost").pathname;
      const rows = path === "/api/mail_server/search" ? servers : [];
      return new Response(JSON.stringify({ value: { rows }, denied: [] }), { status: 200 });
    }),
  );
  act(() => root.render(<MailTab token="tok" notify={() => {}} readsConnections={false} />));
  await settle();
  const refused = container.querySelector("[data-test-outcome=permanent]")!;
  expect(refused.textContent).toContain("Not sent, refused");
  // The server's words are the detail, without obop's own English prefix.
  expect(refused.textContent).toContain("(550 no such user)");
  expect(refused.textContent).not.toContain("failed:");
  expect(refused.className).toContain("mail-error");
  // A server tested before the outcome code existed shows the server's words as before.
  const old = container.querySelector("[data-test-outcome='']")!;
  expect(old.textContent).toContain("sent 250 ok");
  expect(old.className).not.toContain("mail-error");
});

it("words the outcome from the field's option labels, in the language of the page", async () => {
  registerMeta([
    {
      id: "mail",
      models: [
        {
          name: "mail_server",
          fields: [
            {
              name: "last_test_outcome",
              kind: "select",
              options: ["sent", "temporary", "permanent"],
              option_labels: {
                permanent: { en: "Not sent (refused)", de: "Nicht gesendet (abgelehnt)" },
              },
            },
          ],
        },
      ],
    } as unknown as ModuleMeta,
  ]);
  provideCatalogue("de", { "Last test {when}: {result}": "Letzter Test {when}: {result}" });
  await setLanguage("de", false);
  try {
    const servers = [
      {
        id: "s1",
        name: "Relay",
        kind: "smtp",
        host: "smtp.example.org",
        last_test_at: "2026-11-20T10:00:00Z",
        last_test_result: "failed: 550 no such user",
        last_test_outcome: "permanent",
      },
    ];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        const path = new URL(url, "http://localhost").pathname;
        const rows = path === "/api/mail_server/search" ? servers : [];
        return new Response(JSON.stringify({ value: { rows }, denied: [] }), { status: 200 });
      }),
    );
    act(() => root.render(<MailTab token="tok" notify={() => {}} readsConnections={false} />));
    await settle();
    const line = container.querySelector("[data-test-outcome=permanent]")!;
    expect(line.textContent).toContain("Letzter Test");
    expect(line.textContent).toContain("Nicht gesendet (abgelehnt)");
    expect(line.textContent).not.toContain("Not sent");
    expect(line.textContent).toContain("(550 no such user)");
  } finally {
    registerMeta([]);
    provideCatalogue("en", {});
    await setLanguage("en", false);
  }
});
