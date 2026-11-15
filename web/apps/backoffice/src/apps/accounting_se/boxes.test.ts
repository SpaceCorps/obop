// boxes.ts: the sections of the Swedish VAT return and the amount of each box.
import { describe, expect, it } from "vite-plus/test";
import type { Row } from "../../api.ts";
import { boxKey, returnSections } from "./boxes.ts";

const box = (id: string, n: string, extra: Partial<Row> = {}): Row => ({
  id,
  box: n,
  label: `Box ${n}`,
  review: "verified",
  ...extra,
});

describe("returnSections", () => {
  it("groups the boxes by the form's sections, in the form's order, with their amounts", () => {
    const boxes = [
      box("b48", "48"),
      box("b10", "10"),
      box("b05", "5"),
      box("b06", "06", { not_used: true }),
      box("b49", "49"),
      box("b99", "99", { review: "unverified" }),
    ];
    const lines: Row[] = [
      { id: "l1", box_id: "b05", amount: 10000, vouchers: "V1" },
      { id: "l2", box_id: "b05", amount: 500, vouchers: "V2" },
      { id: "l3", box_id: "b10", amount: 2625 },
      { id: "l4", box_id: "b48", amount: 1000 },
      { id: "l5", box_id: "b49", amount: 1 },
    ];
    const s = returnSections(boxes, lines, 1625);
    expect(s.map((x) => x.key)).toEqual(["A", "B", "F", "G", "other"]);
    const a = s[0]!.rows;
    // Box 06 is not used and has no line: left out. "5" is box 05.
    expect(a.map((r) => r.box)).toEqual(["05"]);
    expect(a[0]!.amount).toBe(10500);
    expect(a[0]!.vouchers).toBe("V1\nV2");
    // Box 49 is the report's own VAT to pay, not its line.
    expect(s[3]!.rows[0]!.amount).toBe(1625);
    expect(s[4]!.rows[0]).toMatchObject({ box: "99", filled: false, verified: false });
  });

  it("reads box numbers with or without the leading zero", () => {
    expect(boxKey(5)).toBe("05");
    expect(boxKey("05")).toBe("05");
    expect(boxKey("49")).toBe("49");
  });
});
