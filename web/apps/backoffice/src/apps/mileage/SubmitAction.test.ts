import { describe, expect, test } from "vite-plus/test";
import { offersSubmit } from "./SubmitAction.tsx";

describe("Submit on a trip", () => {
  test("an ended trip that is not private has Submit, a private one has none", () => {
    expect(offersSubmit({ state: "ended", purpose: "business" })).toBe(true);
    expect(offersSubmit({ state: "ended", purpose: "commute" })).toBe(true);
    expect(offersSubmit({ state: "ended", purpose: "private" })).toBe(false);
    expect(offersSubmit({ state: "in_review", purpose: "business" })).toBe(false);
    expect(offersSubmit({ state: "rejected", purpose: "business" })).toBe(false);
  });
});
