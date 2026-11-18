// @vitest-environment jsdom
// The next step of a card on a governed board asks first when there is no way back (#102), and the
// question's way out says Cancel, as the drop and the palette do: nothing is being edited on a board.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, ModuleMeta, Row } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { emptyState } from "../query.ts";
import { KanbanView } from "./KanbanView.tsx";
import type { ViewProps } from "./types.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const state: FieldMeta = {
  name: "state",
  kind: "select",
  description: "",
  class: "none",
  personal_data: false,
  required: true,
  options: ["requested", "approved"],
};
const approve: Command = {
  id: "leave.approve",
  description: "approve",
  needs_approval: false,
  field: "state",
  from: "requested",
  to: "approved",
  ui: { primary: true },
};
const model: ModelMeta = {
  name: "leave",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    {
      name: "title",
      kind: "text",
      description: "",
      class: "none",
      personal_data: false,
      required: true,
    },
    state,
  ],
  actions: [approve],
};
const modules: ModuleMeta[] = [{ id: "hr", name: "HR", models: [model] }];
const rows: Row[] = [{ id: "r1", title: "Summer", state: "requested" }];

let container: HTMLDivElement;
let root: Root;
let confirm: ReturnType<typeof vi.fn>;

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  localStorage.clear();
  confirm = vi.fn(async () => false);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ value: { rows: [], total: 0 } }))),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  const props: ViewProps = {
    model,
    title: "Time off",
    fields: model.fields,
    rows,
    state: { ...emptyState(), group: ["state"] },
    setState: () => {},
    selectedId: "",
    onOpen: () => {},
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
  };
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={modules}
        apps={[]}
        notify={vi.fn() as never}
        confirm={confirm as never}
        linkFor={() => null}
      >
        <KanbanView {...props} />
      </DataProvider>,
    );
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the next step of a card", () => {
  it("asks before a step with no way back, with Cancel as the way out", async () => {
    const next = container.querySelector<HTMLButtonElement>('[data-card="r1"] .knext')!;
    expect(next.textContent).toContain("Approve");
    await act(async () => next.click());
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm.mock.calls[0]![0]).toContain("There is no way back");
    expect(confirm.mock.calls[0]![2]).toBe("Cancel");
  });
});
