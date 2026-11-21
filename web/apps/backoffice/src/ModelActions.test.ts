// U66: a command of the model as a whole (`whole_model`) is offered by the list, never by the
// record panel: `outbound_mail.send` and `process` take no record id.
import { expect, it } from "vite-plus/test";
import type { Command, ModelMeta } from "./api.ts";
import { modelActions } from "./ModelActions.tsx";
import { nextActions } from "./presentation.ts";

const cmd = (id: string, more: Partial<Command> = {}): Command => ({
  id,
  description: "",
  needs_approval: false,
  ...more,
});

const mail: ModelMeta = {
  name: "outbound_mail",
  description: "",
  can_create: false,
  can_update: false,
  can_delete: false,
  fields: [
    {
      name: "status",
      description: "",
      class: "none",
      personal_data: false,
      kind: "select",
      required: false,
      options: ["queued", "sending", "sent", "failed", "cancelled"],
    },
  ],
  actions: [
    cmd("outbound_mail.send", { needs_approval: true, whole_model: true }),
    cmd("outbound_mail.process", { whole_model: true }),
    cmd("outbound_mail.retry", { field: "status", from: "failed", to: "queued" }),
  ],
};

it("leaves the commands of the whole model out of a record's actions", () => {
  const next = nextActions(mail, { id: "m1", status: "failed" });
  const ids = [next.primary, ...next.secondary, ...next.danger]
    .filter((a): a is Command => !!a)
    .map((a) => a.id);
  expect(ids).toContain("outbound_mail.retry");
  expect(ids).not.toContain("outbound_mail.send");
  expect(ids).not.toContain("outbound_mail.process");
});

it("offers them on the list", () => {
  expect(modelActions(mail).map((a) => a.id)).toEqual([
    "outbound_mail.send",
    "outbound_mail.process",
  ]);
});
