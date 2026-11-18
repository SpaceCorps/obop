// A step with no way back asks first even when its package declares no confirm (#102).
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, ModelMeta } from "./api.ts";
import { oneWay, questionOf } from "./workflow.ts";

const act = (id: string, from: string | null, to: string, ui?: Command["ui"]): Command => ({
  id: `shift.${id}`,
  description: "",
  needs_approval: false,
  field: "state",
  from,
  to,
  ui,
});
const model = (actions: Command[]): ModelMeta => ({
  name: "shift",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    {
      name: "state",
      kind: "select",
      description: "",
      class: "none",
      personal_data: false,
      required: true,
      options: ["draft", "published", "confirmed", "cancelled"],
    },
  ],
  actions,
});

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
});

describe("oneWay and questionOf", () => {
  const publish = act("publish", "draft", "published");
  const confirm = act("confirm", "published", "confirmed");
  const cancel = act("cancel", null, "cancelled");
  const reopen = act("reopen", "cancelled", "draft");

  it("finds a step whose state nothing leaves", () => {
    const m = model([publish, confirm]);
    expect(oneWay(m, publish)).toBe(false);
    expect(oneWay(m, confirm)).toBe(true);
    expect(questionOf(m, confirm)).toBe(
      "Confirm? There is no way back: afterwards no action moves it out of Confirmed.",
    );
    expect(questionOf(m, publish)).toBeUndefined();
  });

  it("counts an action from any state as a way out", () => {
    const m = model([publish, confirm, cancel]);
    expect(oneWay(m, confirm)).toBe(false);
    expect(oneWay(m, cancel)).toBe(true);
    expect(oneWay(model([publish, confirm, cancel, reopen]), cancel)).toBe(false);
  });

  it("keeps the package's own question first", () => {
    const asked = act("confirm", "published", "confirmed", { confirm: "Confirm the shift?" });
    expect(questionOf(model([publish, asked]), asked)).toBe("Confirm the shift?");
  });

  it("takes the server's word, which counts the actions of other groups too", () => {
    // An employee's view of a leave request: Request is listed, Approve and Refuse (a manager's)
    // are not, yet the server knows the request has a way out.
    const request = { ...act("request", "draft", "published"), one_way: false };
    expect(oneWay(model([request]), request)).toBe(false);
    expect(questionOf(model([request]), request)).toBeUndefined();
    const lock = { ...act("confirm", "published", "confirmed"), one_way: true };
    expect(oneWay(model([lock, cancel]), lock)).toBe(true);
  });

  it("asks nothing for an action that moves no state", () => {
    const send: Command = { id: "shift.notify", description: "", needs_approval: false };
    expect(questionOf(model([publish, send]), send)).toBeUndefined();
  });
});
