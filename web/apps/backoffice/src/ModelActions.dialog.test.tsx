// @vitest-environment jsdom
// The list's Send of the Mail deliveries (U66), mounted for real: a refusal keeps the dialog open
// with what was typed and says why inside it; only a run that went through closes it.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";

type Answer =
  | { kind: "ok"; value: unknown }
  | { kind: "approval"; planId: string }
  | { kind: "error"; message: string };

const data = {
  run: vi.fn<(id: string, input: Record<string, unknown>) => Promise<Answer>>(),
  notify: vi.fn(),
  refreshAll: vi.fn(),
};
vi.mock("./data.tsx", () => ({ useData: () => data }));

const { ModelActions } = await import("./ModelActions.tsx");

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const text = (name: string): FieldMeta => ({
  name,
  description: "",
  class: "none",
  personal_data: false,
  kind: "text",
  required: false,
});
const send: Command = {
  id: "outbound_mail.send",
  description: "",
  needs_approval: false,
  whole_model: true,
  ui: { needs: ["to", "subject"] },
} as Command;
const mail: ModelMeta = {
  name: "outbound_mail",
  description: "",
  can_create: false,
  can_update: false,
  can_delete: false,
  fields: [text("to"), text("subject")],
  actions: [send],
};

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  data.run.mockReset();
  data.notify.mockReset();
  data.refreshAll.mockReset();
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const settle = () => act(async () => await new Promise((r) => setTimeout(r, 0)));

function type(input: HTMLInputElement, value: string) {
  const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  act(() => {
    set.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

async function openAndSend() {
  act(() => root.render(<ModelActions model={mail} />));
  act(() =>
    (container.querySelector("[data-model-action='outbound_mail.send']") as HTMLElement).click(),
  );
  type(document.getElementById("act-to") as HTMLInputElement, "ada@example.org");
  type(document.getElementById("act-subject") as HTMLInputElement, "Hello");
  const confirm = [...document.querySelectorAll(".action-dialog button")].find(
    (b) => b.textContent === "Send",
  ) as HTMLButtonElement;
  act(() => confirm.click());
  await settle();
}

it("keeps the dialog and what was typed when the server refuses, and says why inside it", async () => {
  data.run.mockResolvedValue({
    kind: "error",
    message: "no mail server is set up: add one under Customize, Mail",
  });
  await openAndSend();
  expect(data.run).toHaveBeenCalledWith("outbound_mail.send", {
    to: "ada@example.org",
    subject: "Hello",
  });
  expect(document.querySelector(".action-dialog")).not.toBeNull();
  expect((document.getElementById("act-to") as HTMLInputElement).value).toBe("ada@example.org");
  expect((document.getElementById("act-subject") as HTMLInputElement).value).toBe("Hello");
  expect(document.querySelector(".action-dialog [role=alert]")?.textContent).toBe(
    "no mail server is set up: add one under Customize, Mail",
  );
  expect(data.notify).not.toHaveBeenCalled();
});

it("closes the dialog once the message is queued", async () => {
  data.run.mockResolvedValue({ kind: "ok", value: "m1" });
  await openAndSend();
  expect(document.querySelector(".action-dialog")).toBeNull();
  expect(data.notify).toHaveBeenCalledTimes(1);
  expect(data.refreshAll).toHaveBeenCalled();
});
