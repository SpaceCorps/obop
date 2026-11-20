// A package's form sections, action questions and done notes follow the language (R17): the
// translations come in /api/meta beside the English text and win over it.
import { afterEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { Command, ModelMeta } from "./api.ts";
import { actionConfirm, actionDone } from "./labels.ts";
import { sectionsOf } from "./recordLayout.ts";
import { questionOf } from "./workflow.ts";

const close: Command = {
  id: "helpdesk_ticket.close",
  description: "",
  needs_approval: false,
  field: "state",
  from: "resolved",
  to: "closed",
  ui: {
    confirm: "Close this ticket for good?",
    confirm_labels: { de: "Dieses Ticket endgültig schließen?" },
    done: "Ticket closed",
    done_labels: { de: "Ticket geschlossen" },
  },
};
const ticket: ModelMeta = {
  name: "helpdesk_ticket",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [
    {
      name: "subject",
      kind: "text",
      description: "",
      class: "none",
      personal_data: false,
      required: true,
    },
  ],
  actions: [close],
  ui: { section: [{ label: "Request", fields: ["subject"], labels: { de: "Anliegen" } }] },
};
const sectionLabels = () =>
  sectionsOf(ticket, ticket.fields, { creating: true, row: null, filled: () => false }).map(
    (s) => s.label,
  );

afterEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
});

describe("package texts in the language of the screen (R17)", () => {
  it("are English without a translation for the language", async () => {
    provideCatalogue("en", {});
    await setLanguage("en", false);
    expect(actionConfirm(close)).toBe("Close this ticket for good?");
    expect(actionDone(close)).toBe("Ticket closed");
    expect(sectionLabels()).toEqual(["Request"]);
  });

  it("take the package's translation", async () => {
    provideCatalogue("de", {});
    await setLanguage("de", false);
    expect(actionConfirm(close)).toBe("Dieses Ticket endgültig schließen?");
    expect(questionOf(ticket, close)).toBe("Dieses Ticket endgültig schließen?");
    expect(actionDone(close)).toBe("Ticket geschlossen");
    expect(sectionLabels()).toEqual(["Anliegen"]);
  });
});
