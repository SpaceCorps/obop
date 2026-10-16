import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";
import { canMove, governedNames, initialOption, isGoverned, pickAction } from "./workflow.ts";

const field = (name: string, kind: FieldMeta["kind"], options?: string[]): FieldMeta => ({
  name,
  kind,
  options,
  description: "",
  class: "none",
  personal_data: false,
  required: false,
});
const act = (id: string, from: string | null, to: string, needs_approval = false): Command => ({
  id: `lead.${id}`,
  description: id,
  needs_approval,
  field: "stage",
  from,
  to,
});

const stage = field("stage", "select", ["new", "qualified", "won", "lost"]);
const model = (actions: Command[]): ModelMeta => ({
  name: "lead",
  description: "",
  can_create: true,
  can_update: true,
  can_delete: true,
  fields: [field("title", "text"), stage],
  actions,
});
const lead = model([
  act("qualify", "new", "qualified"),
  act("mark_won", null, "won", true),
  act("mark_lost", null, "lost"),
  act("reopen", "lost", "new"),
  { id: "lead.export", description: "no transition", needs_approval: false },
]);

describe("governed fields", () => {
  it("a select moved by some action is governed, others are not", () => {
    expect(isGoverned(lead, stage)).toBe(true);
    expect(isGoverned(lead, field("title", "text"))).toBe(false);
    expect(isGoverned(lead, field("other", "select", ["a"]))).toBe(false);
    expect([...governedNames(lead)]).toEqual(["stage"]);
  });

  it("a new record starts at the first option", () => {
    expect(initialOption(stage)).toBe("new");
  });
});

describe("which moves are allowed", () => {
  it("allows exactly the moves a declared action leads to from the current state", () => {
    expect(canMove(lead, stage, "new", "qualified", "Lead").allowed).toBe(true);
    // `from` null means any state.
    expect(canMove(lead, stage, "qualified", "won", "Lead").allowed).toBe(true);
    expect(canMove(lead, stage, "lost", "won", "Lead").allowed).toBe(true);
  });

  it("refuses the rest and says why, naming the record kind and both states", () => {
    const m = canMove(lead, stage, "won", "new", "Lead");
    expect(m.allowed).toBe(false);
    expect(m.reason).toBe("No action moves a lead from Won to New");
    expect(canMove(lead, stage, "qualified", "new", "Lead").allowed).toBe(false);
  });

  it("the current state is not a move", () => {
    const m = canMove(lead, stage, "new", "new", "Lead");
    expect(m).toMatchObject({ allowed: false, current: true, reason: "" });
  });

  it("marks a move that needs approval", () => {
    expect(canMove(lead, stage, "new", "won", "Lead").needsApproval).toBe(true);
    expect(canMove(lead, stage, "new", "lost", "Lead").needsApproval).toBe(false);
  });

  it("only actions listed in meta count: without them nothing can be moved", () => {
    const none = model([]);
    expect(canMove(none, stage, "new", "qualified", "Lead").allowed).toBe(false);
  });

  it("an exact from wins over an open one when two actions lead to the same state", () => {
    const m = model([act("any_to_new", null, "new"), act("reopen", "lost", "new")]);
    expect(pickAction(m, "stage", "lost", "new")?.id).toBe("lead.reopen");
    expect(pickAction(m, "stage", "won", "new")?.id).toBe("lead.any_to_new");
  });
});
