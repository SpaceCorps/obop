// @vitest-environment jsdom
// "Move to…" on a governed field asks what the action button asks (#104, #102): a move that an
// action with a confirm makes never runs unasked by the form's menu.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { markActionOnly, type FieldMeta, type ModelMeta, type ModuleMeta } from "./api.ts";
import { DataProvider } from "./data.tsx";
import { RecordPanel } from "./RecordPanel.tsx";

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
const box: ModelMeta = {
  name: "vat_box",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: false,
  fields: [
    field("code", { required: true }),
    field("review", { kind: "select", required: true, options: ["unverified", "verified"] }),
  ],
  actions: [
    {
      id: "vat_box.verify",
      description: "Verify",
      needs_approval: false,
      field: "review",
      from: "unverified",
      to: "verified",
      ui: { confirm: "Mark this box as verified? It cannot be moved back." },
    },
  ],
};
const modules: ModuleMeta[] = [{ id: "vat", name: "VAT", models: [box] }];
markActionOnly(modules);

let container: HTMLDivElement;
let root: Root;
let calls: string[];

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  calls = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      calls.push(String(url).replace(/\?.*$/, ""));
      return new Response(JSON.stringify({ value: { id: "b1", rows: [], total: 0 } }));
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

describe("Move to… on a governed field", () => {
  it("asks the action's question first and runs nothing until it is answered", async () => {
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
          <RecordPanel
            model={box}
            fields={box.fields}
            row={{ id: "b1", code: "06", review: "unverified" }}
            defaults={{}}
            withheld={[]}
            title="VAT box"
            onClose={() => {}}
            onSaved={() => {}}
          />
        </DataProvider>,
      );
    });
    const trigger = [...container.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("Move to…"),
    );
    expect(trigger).toBeTruthy();
    await act(async () => trigger!.click());
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((b) =>
      b.textContent?.includes("Verified"),
    );
    expect(item).toBeTruthy();
    await act(async () => item!.click());
    expect(calls.filter((u) => u.includes("/api/vat_box/"))).toEqual([]);
    expect(document.body.textContent).toContain("Mark this box as verified?");
  });
});
