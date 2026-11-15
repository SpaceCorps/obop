// @vitest-environment jsdom
// The list view, mounted (lane L2): a click anywhere in a row opens that row's record (a ref's words
// included), a cell that can change waits a moment for a double-click, which edits it in place, and
// the footer adds up only the columns that add up.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setInstanceDefaults, setLanguage } from "@obop/ui";
import type { FieldMeta, ModelMeta, ModuleMeta, Row } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { emptyState } from "../query.ts";
import { ListView } from "./ListView.tsx";
import type { ViewProps } from "./types.ts";

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
const model: ModelMeta = {
  name: "deal",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    field("title", { required: true }),
    field("customer_id", { kind: "ref", to: "party" }),
    field("amount_minor", { kind: "money" }),
    field("unit_price_minor", { kind: "money" }),
    field("level", { kind: "number" }),
  ],
  actions: [],
};
const party: ModelMeta = {
  name: "party",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [field("name", { required: true })],
  actions: [],
};
const modules: ModuleMeta[] = [
  { id: "deals", name: "Deals", models: [model] },
  { id: "base", name: "Contacts", models: [party] },
];
const rows: Row[] = [
  {
    id: "d1",
    title: "First deal",
    customer_id: "p1",
    amount_minor: 1000,
    unit_price_minor: 5,
    level: 3,
  },
  {
    id: "d2",
    title: "Second deal",
    customer_id: "p1",
    amount_minor: 250,
    unit_price_minor: 5,
    level: 4,
  },
];

let container: HTMLDivElement;
let root: Root;
let onOpen: ReturnType<typeof vi.fn>;

async function mount(over: Partial<ViewProps> = {}, m: ModelMeta = model) {
  const props: ViewProps = {
    model: m,
    title: "Deal",
    fields: m.fields,
    rows,
    state: emptyState(),
    setState: () => {},
    selectedId: "",
    onOpen: onOpen as never,
    canCreate: true,
    canUpdate: true,
    update: async () => null,
    create: async () => null,
    moveState: async () => null,
    total: rows.length,
    fetchAll: async () => ({ rows: [], total: 0, complete: true }) as never,
    canDelete: true,
    setDialog: () => {},
    newWith: () => {},
    drill: () => {},
    ...over,
  };
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={modules}
        apps={[]}
        notify={() => {}}
        confirm={async () => true}
        linkFor={(name, o) => `#/x/${name}?id=${o?.id ?? ""}`}
      >
        <ListView {...props} />
      </DataProvider>,
    );
  });
}

const rowOf = (title: string) =>
  [...container.querySelectorAll<HTMLTableRowElement>("tbody tr")].find((r) =>
    r.textContent?.includes(title),
  )!;
const click = (el: Element, detail = 1) =>
  act(async () => {
    el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail }));
  });

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  await setInstanceDefaults({ currency: "EUR" });
  localStorage.clear();
  onOpen = vi.fn();
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({ value: { rows: [{ id: "p1", name: "Baumann Bau" }], total: 1 } }),
        ),
    ),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("a click in the list", () => {
  it("on a ref's words opens the row's record, and the link is not followed", async () => {
    await mount({ canUpdate: false });
    const label = rowOf("Second deal").querySelector(".reftext-label")!;
    const ev = new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 });
    await act(async () => void label.dispatchEvent(ev));
    expect(ev.defaultPrevented).toBe(true);
    expect(onOpen).toHaveBeenCalledWith("d2");
  });

  it("on the title opens the record at once, also for a person who may edit", async () => {
    await mount();
    await click(rowOf("First deal").querySelector("td[data-c='1']")!);
    expect(onOpen).toHaveBeenCalledWith("d1");
  });

  it("on a cell that can change waits for a second click, which edits it in place", async () => {
    vi.useFakeTimers();
    await mount();
    const cell = rowOf("First deal").querySelector("td[data-f='level']")!;
    await click(cell, 1);
    expect(onOpen).not.toHaveBeenCalled();
    await click(cell, 2);
    await act(async () => void vi.advanceTimersByTime(400));
    expect(onOpen).not.toHaveBeenCalled();
    expect(container.querySelector("td.editing input")).not.toBeNull();
    // A single click opens the record once the moment has passed.
    await click(rowOf("Second deal").querySelector("td[data-f='level']")!, 1);
    await act(async () => void vi.advanceTimersByTime(400));
    expect(onOpen).toHaveBeenCalledWith("d2");
  });
});

describe("the footer", () => {
  it("adds up the amount, not a unit price or a level", async () => {
    await mount();
    const foot = (f: string) =>
      container.querySelector(`tfoot td[data-f="${f}"]`)?.textContent ?? null;
    expect(foot("amount_minor")).toBe("€12.50");
    expect(foot("unit_price_minor")).toBe("");
    expect(foot("level")).toBe("");
  });

  it("is not there when nothing adds up", async () => {
    const plain: ModelMeta = { ...model, fields: [model.fields[0]!, model.fields[4]!] };
    await mount({}, plain);
    expect(container.querySelector("tfoot")).toBeNull();
  });
});
