import { describe, expect, it } from "vite-plus/test";
import { lastMonth } from "./BillTime.tsx";

describe("Bill time", () => {
  it("offers the month before as the period", () => {
    expect(lastMonth(new Date(2026, 10, 17))).toEqual({ from: "2026-10-01", to: "2026-10-31" });
    // January looks back into December of the year before.
    expect(lastMonth(new Date(2026, 0, 5))).toEqual({ from: "2025-12-01", to: "2025-12-31" });
    expect(lastMonth(new Date(2028, 2, 1))).toEqual({ from: "2028-02-01", to: "2028-02-29" });
  });
});
