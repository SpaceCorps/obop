// @vitest-environment jsdom
// #109: a provider policy saved from any form (the generic one too) reloads the providers it
// enables, so the connectors wizard unlocks the provider without a page reload.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { ModelMeta, ModuleMeta } from "./api.ts";
import { DataProvider, useData } from "./data.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const model = (name: string): ModelMeta => ({
  name,
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    {
      name: "name",
      kind: "text",
      description: "",
      class: "none",
      personal_data: false,
      required: false,
    },
  ],
  actions: [],
});
const modules: ModuleMeta[] = [
  {
    id: "connectors",
    name: "Connectors",
    models: [model("provider_policy"), model("connector_provider")],
  },
];

let root: Root;
let container: HTMLDivElement;
let calls: string[];
let api: ReturnType<typeof useData> | null;

function Probe() {
  api = useData();
  return null;
}

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  calls = [];
  api = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      calls.push(String(url).replace(/\?.*$/, ""));
      return new Response(JSON.stringify({ value: { id: "x", rows: [], total: 0 } }));
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

it("a write to provider_policy reloads the cached connector providers (#109)", async () => {
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
        <Probe />
      </DataProvider>,
    );
  });
  await act(async () => {
    await api!.store.load("connector_provider");
  });
  const before = calls.filter((u) => u.includes("connector_provider")).length;
  expect(before).toBeGreaterThan(0);
  await act(async () => {
    await api!.run("provider_policy.create", { provider: "webhook_out" });
  });
  expect(calls.filter((u) => u.includes("connector_provider")).length).toBeGreaterThan(before);
});
