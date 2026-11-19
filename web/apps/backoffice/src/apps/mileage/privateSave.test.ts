import { describe, expect, it } from "vite-plus/test";
import { recordExtensionOf } from "../../extensions.ts";
import "../../registrations.ts";
import { privateSaveQuestion } from "./privateSave.ts";

describe("saving a trip as private", () => {
  it("asks when the purpose becomes private", () => {
    expect(privateSaveQuestion({ id: "t1", purpose: "business" }, { purpose: "private" })).toMatch(
      /removes the route for good/,
    );
  });
  it("does not ask for any other change", () => {
    expect(
      privateSaveQuestion({ id: "t1", purpose: "business" }, { vehicle: "HH-W 2" }),
    ).toBeNull();
    expect(
      privateSaveQuestion({ id: "t1", purpose: "private" }, { purpose: "private" }),
    ).toBeNull();
    expect(
      privateSaveQuestion({ id: "t1", purpose: "private" }, { purpose: "business" }),
    ).toBeNull();
  });
  it("is what the trip form asks before it saves", () => {
    const asks = recordExtensionOf("mileage_trip").confirmSave;
    expect(asks.map((a) => a({ id: "t1", purpose: "business" }, { purpose: "private" }))).toEqual([
      privateSaveQuestion({ id: "t1", purpose: "business" }, { purpose: "private" }),
    ]);
  });
});
