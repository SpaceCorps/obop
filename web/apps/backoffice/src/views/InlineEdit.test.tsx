// @vitest-environment jsdom
// The cell editor of the list (#87, #98): leaving the cell saves what was typed, Escape cancels,
// and an unset switch saved as No writes an explicit false.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { FieldMeta, Row } from "../api.ts";
import { DataProvider } from "../data.tsx";
import { InlineEdit } from "./InlineEdit.tsx";

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

let container: HTMLDivElement;
let root: Root;
let outside: HTMLButtonElement;

async function mount(f: FieldMeta, row: Row) {
  const update = vi.fn(async () => null as string | null);
  const done = vi.fn();
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={[]}
        apps={[]}
        notify={() => {}}
        confirm={async () => true}
        linkFor={() => "#/"}
      >
        <InlineEdit field={f} row={row} update={update} done={done} />
      </DataProvider>,
    );
  });
  return { update, done };
}

const input = () => container.querySelector<HTMLInputElement>("input")!;
const type = (v: string) =>
  act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(input(), v);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });
const key = (k: string) =>
  act(async () => {
    input().dispatchEvent(
      new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }),
    );
  });
const leave = () =>
  act(async () => {
    outside.focus();
  });

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  container = document.createElement("div");
  outside = document.createElement("button");
  document.body.append(container, outside);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  outside.remove();
});

describe("InlineEdit", () => {
  it("saves a changed value when the cell is left by a click elsewhere", async () => {
    const { update, done } = await mount(field("email"), { id: "r1", email: "old@x.test" });
    input().focus();
    await type("new@x.test");
    await leave();
    expect(update).toHaveBeenCalledWith("r1", { email: "new@x.test" });
    expect(done).toHaveBeenCalled();
  });

  it("Escape cancels: the blur that follows saves nothing", async () => {
    const { update, done } = await mount(field("email"), { id: "r1", email: "old@x.test" });
    input().focus();
    await type("never@x.test");
    await key("Escape");
    await leave();
    expect(update).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalled();
  });

  it("leaving an unchanged cell writes nothing", async () => {
    const { update, done } = await mount(field("active", { kind: "bool" }), { id: "r1" });
    input().focus();
    await leave();
    expect(update).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalled();
  });

  it("Enter on an unset switch shown as No writes an explicit false", async () => {
    const { update } = await mount(field("active", { kind: "bool" }), { id: "r1", active: null });
    input().focus();
    await key("Enter");
    expect(update).toHaveBeenCalledWith("r1", { active: false });
  });

  it("Enter on a switch already No writes nothing", async () => {
    const { update, done } = await mount(field("active", { kind: "bool" }), {
      id: "r1",
      active: false,
    });
    input().focus();
    await key("Enter");
    expect(update).not.toHaveBeenCalled();
    expect(done).toHaveBeenCalled();
  });
});
