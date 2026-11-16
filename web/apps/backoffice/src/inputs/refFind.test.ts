import { describe, expect, it } from "vite-plus/test";
import type { Row } from "../api.ts";
import { byName, existing, nameKey, searchWord } from "./refFind.ts";

const party = (id: string, name: string): Row => ({ id, name });

describe("a typed name finds the record it means", () => {
  const rows = [
    party("baumann", "Baumann Bau GmbH"),
    party("petersen", "Elektro Petersen GmbH"),
    party("d1", "Doppel Kontakt GmbH"),
    party("d2", "Doppel Kontakt GmbH"),
  ];

  it("ignores case and repeated spaces", () => {
    expect(nameKey("  Elektro  Petersen   GmbH ")).toBe("elektro petersen gmbh");
    expect(byName(rows, "name", "baumann bau gmbh", true)).toEqual({ kind: "one", id: "baumann" });
    expect(byName(rows, "name", "Elektro  Petersen GmbH", true)).toEqual({
      kind: "one",
      id: "petersen",
    });
  });

  it("does not choose between two records of one name", () => {
    expect(byName(rows, "name", "Doppel Kontakt GmbH", true)).toEqual({ kind: "several" });
  });

  it("finds none only when the server gave every match", () => {
    expect(byName(rows, "name", "Zzz Neu", true)).toEqual({ kind: "none" });
    expect(byName(rows, "name", "Zzz Neu", false)).toEqual({ kind: "several" });
  });

  it("asks the server for the longest word", () => {
    expect(searchWord("Elektro  Petersen GmbH")).toBe("petersen");
  });
});

describe("the record that already stands for a contact", () => {
  it("is the active one, else the archived one", () => {
    expect(
      existing([
        { id: "old", active: false },
        { id: "new", active: true },
      ]),
    ).toEqual({ kind: "active", id: "new" });
    expect(existing([{ id: "old", active: false }])).toEqual({ kind: "archived", id: "old" });
    expect(existing([])).toEqual({ kind: "none" });
  });
});
