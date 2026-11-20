// @vitest-environment jsdom
// What a person is told when an act made several plans at once (several deletes proposed by an
// agent's token), and what a screen reader hears on the links of a result list (D97, rule 10: a
// toast and a result list only link).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { ModelMeta, ModuleMeta } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { DeleteDialog } from "../DeleteDialog.tsx";
import { notifySentMany } from "./sent.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const lead: ModelMeta = {
  name: "lead",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [],
  actions: [],
};
const modules: ModuleMeta[] = [{ id: "crm", name: "CRM", models: [lead] }];

let container: HTMLDivElement;
let root: Root;
let notify: ReturnType<typeof vi.fn>;
let answers: Record<string, { status: number; body: unknown }>;

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  notify = vi.fn();
  answers = {};
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_url: string, init?: RequestInit) => {
      const id = JSON.parse(String(init?.body ?? "{}")).id as string;
      const a = answers[id] ?? { status: 200, body: { value: null, denied: [] } };
      return new Response(JSON.stringify(a.body), {
        status: a.status,
        headers: { "content-type": "application/json" },
      });
    }),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

const settle = () => act(async () => new Promise((r) => setTimeout(r, 10)));

async function deleteThem(records: { id: string; label: string }[]) {
  await act(async () => {
    root.render(
      <DataProvider
        token="agent-token"
        modules={modules}
        apps={[]}
        notify={notify as never}
        confirm={async () => true}
        linkFor={() => null}
      >
        <DeleteDialog
          model={lead}
          noun="lead"
          records={records}
          onClose={() => {}}
          onDone={() => {}}
        />
      </DataProvider>,
    );
  });
  const confirm = [...document.querySelectorAll("button")].find((b) =>
    /^Delete( \d+ leads)?$/.test(b.textContent ?? ""),
  )!;
  await act(async () => confirm.click());
  await settle();
}

describe("several plans from one act", () => {
  it("one toast says how many and links to the list, not to the first plan", async () => {
    answers = {
      a: { status: 202, body: { plan_id: "plan-a" } },
      b: { status: 202, body: { plan_id: "plan-b" } },
      c: { status: 202, body: { plan_id: "plan-c" } },
    };
    await deleteThem([
      { id: "a", label: "Acme" },
      { id: "b", label: "Beta" },
      { id: "c", label: "Cato" },
    ]);
    expect(notify).toHaveBeenCalledTimes(1);
    const [text, , opts] = notify.mock.calls[0]!;
    expect(text).toBe("3 requests sent for approval.");
    expect(opts.action).toEqual({ label: "Open Approvals", href: "#/approvals" });
  });

  it("a single plan still says one sentence and links to that plan", async () => {
    answers = { a: { status: 202, body: { plan_id: "plan-a" } } };
    await deleteThem([{ id: "a", label: "Acme" }]);
    const [text, , opts] = notify.mock.calls[0]!;
    expect(text).toBe("Sent for approval.");
    expect(opts.action.href).toBe("#/approvals?id=plan-a");
  });

  it("the toast sentence has a form for one, too", () => {
    const notifyOne = vi.fn();
    notifySentMany(notifyOne, 1);
    expect(notifyOne.mock.calls[0]![0]).toBe("1 request sent for approval.");
  });

  it("in the result list each link's name includes its row's label", async () => {
    answers = {
      a: { status: 202, body: { plan_id: "plan-a" } },
      b: { status: 200, body: { value: null, denied: [] } },
      c: { status: 202, body: { plan_id: "plan-c" } },
    };
    await deleteThem([
      { id: "a", label: "Acme" },
      { id: "b", label: "Beta" },
      { id: "c", label: "Cato" },
    ]);
    const links = [...document.querySelectorAll<HTMLAnchorElement>(".results a")];
    expect(links.map((a) => a.getAttribute("aria-label"))).toEqual([
      "Acme, waiting for approval",
      "Cato, waiting for approval",
    ]);
    expect(links.map((a) => a.textContent)).toEqual([
      "waiting for approval",
      "waiting for approval",
    ]);
  });
});
