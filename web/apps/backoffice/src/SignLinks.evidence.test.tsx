// @vitest-environment jsdom
// The evidence check of a signed request (docs/ISSUES.md O3, O10): the panel asks
// `sign_request.get`, which works `evidence_check` out (a list row does not carry it), and shows it
// with its tone; nothing for a request that is not signed, or for a person the server does not show
// the field to.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { type FieldMeta, type ModelMeta, type ModuleMeta, type Row } from "./api.ts";
import { DataProvider } from "./data.tsx";
import { SignEvidenceCheck } from "./SignLinks.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const field = (name: string, extra: Partial<FieldMeta> = {}): FieldMeta => ({
  name,
  kind: "text",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
  ...extra,
});
const check = field("evidence_check", {
  kind: "select",
  options: ["matches", "differs", "no_entry"],
});
const model = (withCheck: boolean): ModelMeta => ({
  name: "sign_request",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [field("title"), field("state"), ...(withCheck ? [check] : [])],
  actions: [],
});

let container: HTMLDivElement;
let root: Root;
let gets: number;
let answer: string | null;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  gets = 0;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      if (String(url).startsWith("/api/sign_request/get")) gets++;
      return new Response(
        JSON.stringify({ value: { id: "r1", state: "signed", evidence_check: answer } }),
        { headers: { "content-type": "application/json" } },
      );
    }),
  );
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

async function mount(row: Row, withCheck = true) {
  const modules: ModuleMeta[] = [{ id: "sign", name: "Sign", models: [model(withCheck)] }];
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={modules}
        apps={[]}
        notify={() => {}}
        confirm={async () => true}
        linkFor={() => null}
      >
        <SignEvidenceCheck request={row} />
      </DataProvider>,
    );
  });
  for (let i = 0; i < 3; i++)
    await act(async () => {
      await new Promise((done) => setTimeout(done, 0));
    });
}

const shown = () =>
  (container.querySelector(".sign-evidence-check")?.textContent ?? "").replace(/\s+/g, " ");

describe("SignEvidenceCheck", () => {
  it("shows what get answers for a signed request, with its tone", async () => {
    answer = "matches";
    await mount({ id: "r1", state: "signed" });
    expect(gets).toBe(1);
    expect(shown()).toMatch(/Evidence check/i);
    expect(shown()).toMatch(/Matches/);
    expect(
      container.querySelector(".sign-evidence-check .badge, .sign-evidence-check [class*=success]"),
    ).not.toBeNull();
  });
  it("says when the evidence was changed after signing", async () => {
    answer = "differs";
    await mount({ id: "r1", state: "signed" });
    expect(shown()).toMatch(/changed after signing/);
  });
  it("asks nothing for a request that is not signed", async () => {
    answer = "matches";
    await mount({ id: "r1", state: "sent" });
    expect(gets).toBe(0);
    expect(container.querySelector(".sign-evidence-check")).toBeNull();
  });
  it("shows nothing to a person the server does not show the field to", async () => {
    answer = "matches";
    await mount({ id: "r1", state: "signed" }, false);
    expect(gets).toBe(0);
    expect(container.querySelector(".sign-evidence-check")).toBeNull();
  });
});
