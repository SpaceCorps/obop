import { describe, expect, it } from "vite-plus/test";
import type { Command, FieldMeta, ModelMeta } from "./api.ts";
import {
  canMove,
  governedNames,
  hiddenForRecord,
  initialOption,
  isGoverned,
  notForRecord,
  pickAction,
  moveRecord,
} from "./workflow.ts";

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

describe("two actions that send a sign request (D92)", () => {
  const state = field("state", "select", ["draft", "sent", "signed", "declined", "expired"]);
  const move = (id: string, from: string | null, to: string, needs_approval = true): Command => ({
    id: `sign_request.${id}`,
    description: id,
    needs_approval,
    field: "state",
    from,
    to,
  });
  const request = (actions: Command[]): ModelMeta => ({
    name: "sign_request",
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields: [state],
    actions,
  });
  // The manifest lists `send` before `send_document`: both lead from draft to sent.
  const both = request([
    move("send", "draft", "sent"),
    move("sign", "sent", "signed"),
    move("send_document", "draft", "sent"),
  ]);

  it("runs send_document for a request about a document, send for one made from a template", () => {
    const doc = { id: "r1", state: "draft", document_id: "d1", template_id: "" };
    const tpl = { id: "r2", state: "draft", document_id: "", template_id: "t1" };
    expect(pickAction(both, "state", "draft", "sent", doc)?.id).toBe("sign_request.send_document");
    expect(pickAction(both, "state", "draft", "sent", tpl)?.id).toBe("sign_request.send");
    // A request with both is about a document: the door needs the copy that send_document keeps.
    const bothSet = { id: "r3", state: "draft", document_id: "d1", template_id: "t1" };
    expect(pickAction(both, "state", "draft", "sent", bothSet)?.id).toBe(
      "sign_request.send_document",
    );
  });

  it("keeps the first action when no record is given, as before", () => {
    expect(pickAction(both, "state", "draft", "sent")?.id).toBe("sign_request.send");
  });

  it("uses the one action there is when the person may run only one of them", () => {
    const onlySend = request([move("send", "draft", "sent")]);
    const onlyDoc = request([move("send_document", "draft", "sent")]);
    const doc = { id: "r1", state: "draft", document_id: "d1" };
    const tpl = { id: "r2", state: "draft", template_id: "t1" };
    expect(pickAction(onlySend, "state", "draft", "sent", doc)?.id).toBe("sign_request.send");
    expect(pickAction(onlyDoc, "state", "draft", "sent", tpl)?.id).toBe(
      "sign_request.send_document",
    );
  });

  it("does not touch another model that has two actions on one move", () => {
    const m = { ...both, name: "lead" };
    const doc = { id: "r1", state: "draft", document_id: "d1" };
    expect(pickAction(m, "state", "draft", "sent", doc)?.id).toBe("sign_request.send");
  });

  it("is what the status bar and the kanban ask: canMove takes the record", () => {
    const doc = { id: "r1", state: "draft", document_id: "d1" };
    const mv = canMove(both, state, "draft", "sent", "Request", doc);
    expect(mv.allowed).toBe(true);
    expect(mv.action?.id).toBe("sign_request.send_document");
    expect(mv.needsApproval).toBe(true);
  });

  it("runs the right command when a card is dragged or the Sent step is clicked", async () => {
    const ran: string[] = [];
    const run = async (id: string) => {
      ran.push(id);
      return { kind: "ok" as const, value: { id: "r1" }, denied: [] };
    };
    const doc = { id: "r1", state: "draft", document_id: "d1" };
    await moveRecord(run, both, state, doc, "sent", "Request");
    expect(ran).toEqual(["sign_request.send_document"]);
  });

  it("greys out the action that does not fit the record, with the reason, and only that one", () => {
    const doc = { id: "r1", state: "draft", document_id: "d1", template_id: "" };
    const tpl = { id: "r2", state: "draft", document_id: "", template_id: "t1" };
    const send = both.actions[0]!;
    const sendDoc = both.actions[2]!;
    const sign = both.actions[1]!;
    expect(notForRecord(both, send, doc)).toBe(
      "This request is about a document. Use Send document.",
    );
    expect(notForRecord(both, sendDoc, doc)).toBe("");
    expect(notForRecord(both, send, tpl)).toBe("");
    expect(notForRecord(both, sendDoc, tpl)).toBe(
      "Send document needs a document. Choose one first.",
    );
    // Marking it signed by hand is for a request that is not about a document (next tests).
    expect(notForRecord(both, sign, tpl)).toBe("");
    // A template request that also has a document may still use send (the server decides).
    const bothSet = { id: "r3", state: "draft", document_id: "d1", template_id: "t1" };
    expect(notForRecord(both, send, bothSet)).toBe("");
    // Another model is never touched.
    expect(notForRecord({ ...both, name: "lead" }, send, doc)).toBe("");
  });
});

describe("marking a sign request signed by hand (D92)", () => {
  const state = field("state", "select", ["draft", "sent", "signed", "declined", "expired"]);
  const sign: Command = {
    id: "sign_request.sign",
    description: "Record by hand that the signer signed",
    needs_approval: true,
    field: "state",
    from: "sent",
    to: "signed",
  };
  const decline: Command = { ...sign, id: "sign_request.decline", to: "declined" };
  const model: ModelMeta = {
    name: "sign_request",
    description: "",
    can_create: true,
    can_update: true,
    can_delete: true,
    fields: [state],
    actions: [sign, decline],
  };
  const why =
    "A request about a document is signed by the signer through the signing link. It cannot be marked signed by hand.";
  const doc = { id: "r1", state: "sent", document_id: "d1", template_id: "" };
  // The document was cleared after sending: only the hash the server took is left.
  const cleared = {
    id: "r2",
    state: "sent",
    document_id: null,
    document_hash: "ab12",
    template_id: "",
  };
  // A request from a template, with a hash typed by hand under sign 0.2.0: staff still close it by hand.
  const legacy = {
    id: "r3",
    state: "sent",
    document_id: "",
    document_hash: "ab12",
    template_id: "t1",
  };
  const template = { id: "r4", state: "sent", document_id: "", template_id: "t1" };
  const blank = { id: "r5", state: "sent" };

  it("has no manual action for a request about a document, or one that holds a document's hash", () => {
    expect(hiddenForRecord(model, sign, doc)).toBe(true);
    expect(hiddenForRecord(model, sign, cleared)).toBe(true);
    // A request with a document and a template is about the document.
    expect(hiddenForRecord(model, sign, { ...doc, template_id: "t1" })).toBe(true);
  });

  it("keeps it for a request from a template, with or without a hash typed by hand, and for a blank one", () => {
    expect(hiddenForRecord(model, sign, legacy)).toBe(false);
    expect(hiddenForRecord(model, sign, template)).toBe(false);
    expect(hiddenForRecord(model, sign, blank)).toBe(false);
  });

  it("hides only that action, only on that model", () => {
    expect(hiddenForRecord(model, decline, doc)).toBe(false);
    expect(hiddenForRecord({ ...model, name: "lead" }, sign, doc)).toBe(false);
  });

  it("says in plain words why the step is not there, where it is still drawn", () => {
    expect(notForRecord(model, sign, doc)).toBe(why);
    expect(notForRecord(model, sign, cleared)).toBe(why);
    expect(notForRecord(model, sign, legacy)).toBe("");
    expect(notForRecord(model, sign, template)).toBe("");
    expect(notForRecord(model, decline, doc)).toBe("");
  });

  it("is not a move the status bar or the kanban can make for such a request", () => {
    const mv = canMove(model, state, "sent", "signed", "Request", doc);
    expect(mv.allowed).toBe(false);
    expect(mv.reason).toBe(why);
    expect(canMove(model, state, "sent", "signed", "Request", cleared).allowed).toBe(false);
    // The other steps of the same request are unaffected.
    expect(canMove(model, state, "sent", "declined", "Request", doc).allowed).toBe(true);
    // And without the record nothing changes.
    expect(canMove(model, state, "sent", "signed", "Request").allowed).toBe(true);
  });

  it("is still a move for a request from a template", () => {
    for (const r of [legacy, template, blank]) {
      const mv = canMove(model, state, "sent", "signed", "Request", r);
      expect(mv.allowed).toBe(true);
      expect(mv.action?.id).toBe("sign_request.sign");
    }
  });

  it("sends nothing when it is run anyway, and says why", async () => {
    const ran: string[] = [];
    const run = async (id: string) => {
      ran.push(id);
      return { kind: "ok" as const, value: { id: "r1" }, denied: [] };
    };
    const out = await moveRecord(run, model, state, doc, "signed", "Request");
    expect(ran).toEqual([]);
    expect(out).toEqual({ kind: "error", message: why, raw: why });
    await moveRecord(run, model, state, template, "signed", "Request");
    expect(ran).toEqual(["sign_request.sign"]);
  });
});
