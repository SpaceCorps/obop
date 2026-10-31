import { describe, expect, it } from "vite-plus/test";
import { remember, remembered, type Last } from "./lastRecord.ts";

const row = (id: string, title = id) => ({ id, title });
const a: Last = { model: "note", id: "A", row: row("A", "old A") };

describe("the row useRecord keeps for the open record", () => {
  it("keeps the last row seen, and takes a newer one", () => {
    expect(remember(null, { model: "note", id: "A", row: row("A") })).toEqual({
      model: "note",
      id: "A",
      row: row("A"),
    });
    expect(remember(a, { model: "note", id: "A", row: row("A", "new A") })?.row).toEqual(
      row("A", "new A"),
    );
  });

  it("keeps it while the get is still on its way, so the open panel is not unmounted", () => {
    expect(remember(a, { model: "note", id: "A", row: undefined })).toBe(a);
  });

  it("forgets it when the get does not return the record: gone, no access, or failed", () => {
    expect(remember(a, { model: "note", id: "A", row: null })).toBeNull();
  });

  it("forgets it when the route holds no record, so Back does not show a deleted one again", () => {
    expect(remember(a, { model: "note", id: "", row: undefined })).toBeNull();
    expect(remember(a, { model: "note", id: "new", row: undefined })).toBeNull();
  });

  it("gives the row back only for the record, and the model, it was seen for", () => {
    expect(remembered(a, "note", "A")).toEqual(row("A", "old A"));
    expect(remembered(a, "note", "B")).toBeUndefined();
    expect(remembered(a, "other", "A")).toBeUndefined();
    expect(remembered(null, "note", "A")).toBeUndefined();
  });
});
