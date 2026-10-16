import { describe, expect, it } from "vite-plus/test";
import type { Reply } from "./api.ts";
import { deleteRecords, parseReferences, plural, summarize } from "./remove.ts";

describe("delete refusal", () => {
  it("reads the referencing models, counts and ids from the server's message", () => {
    const msg =
      "cannot delete party P: still referenced by accounting_move (3): id1, id2, id3; sale_order (1): id4";
    expect(parseReferences(msg)).toEqual([
      { model: "accounting_move", count: 3, ids: ["id1", "id2", "id3"] },
      { model: "sale_order", count: 1, ids: ["id4"] },
    ]);
  });

  it("returns null for any other message", () => {
    expect(parseReferences("lead.title is required")).toBeNull();
  });

  it("plurals read naturally", () => {
    expect(plural("lead", 1)).toBe("lead");
    expect(plural("lead", 3)).toBe("leads");
    expect(plural("party", 2)).toBe("parties");
    expect(plural("box", 2)).toBe("boxes");
    expect(plural("day", 2)).toBe("days");
  });
});

describe("delete many", () => {
  const records = [
    { id: "a", label: "A" },
    { id: "b", label: "B" },
    { id: "c", label: "C" },
    { id: "d", label: "D" },
  ];
  const replies: Record<string, Reply> = {
    a: { kind: "ok", value: { id: "a" }, denied: [] },
    b: {
      kind: "error",
      status: 400,
      message: "cannot delete lead B: still referenced by x (1): i",
    },
    c: { kind: "approval", planId: "0123456789abcdef" },
    d: { kind: "error", status: 400, message: "boom" },
  };

  it("gives every record its own result and keeps going after a failure", async () => {
    const res = await deleteRecords(
      async (_id, body) => replies[(body as { id: string }).id]!,
      "lead",
      records,
    );
    expect(res.map((r) => r.status)).toEqual(["deleted", "referenced", "approval", "failed"]);
    expect(res[1]!.refs).toEqual([{ model: "x", count: 1, ids: ["i"] }]);
    expect(res[2]!.planId).toBe("0123456789abcdef");
    expect(summarize(res, "lead")).toBe(
      "1 lead deleted, 1 waiting for a human to approve (nothing deleted yet), 1 still in use, 1 failed",
    );
  });
});
