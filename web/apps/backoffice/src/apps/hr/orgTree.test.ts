// orgTree.ts: the org chart's tree on `reports_to_id`.
import { describe, expect, it } from "vite-plus/test";
import type { Row } from "../../api.ts";
import { openToDepth, orgTree, pathsTo } from "./orgTree.ts";

const p = (id: string, name: string, boss = "", job = ""): Row => ({
  id,
  name,
  reports_to_id: boss || null,
  job_title: job,
  department_id: "d1",
});
const dept = () => "Sales";

describe("orgTree", () => {
  it("puts people under their manager, sorted by name, and counts everyone below", () => {
    const roots = orgTree(
      [p("k", "Katrin"), p("f", "Frank", "k"), p("a", "Anna", "k"), p("j", "Julia", "f")],
      dept,
    );
    expect(roots.map((r) => r.name)).toEqual(["Katrin"]);
    expect(roots[0]!.children.map((c) => c.name)).toEqual(["Anna", "Frank"]);
    expect(roots[0]!.below).toBe(3);
    expect(roots[0]!.children[1]!.children[0]!.department).toBe("Sales");
  });

  it("draws a person whose manager is not readable at the top", () => {
    const roots = orgTree([p("a", "Anna", "gone"), p("b", "Ben")], dept);
    expect(roots.map((r) => r.name)).toEqual(["Anna", "Ben"]);
  });

  it("cuts a loop of managers instead of losing the people in it", () => {
    const roots = orgTree([p("a", "Anna", "b"), p("b", "Ben", "a"), p("c", "Cleo", "b")], dept);
    expect(roots.map((r) => r.name)).toEqual(["Anna"]);
    expect(roots[0]!.below).toBe(2);
  });

  it("opens the first levels, and the way to a search hit", () => {
    const roots = orgTree(
      [p("k", "Katrin"), p("f", "Frank", "k"), p("j", "Julia", "f", "Accountant")],
      dept,
    );
    expect([...openToDepth(roots, 1)]).toEqual(["k"]);
    const found = pathsTo(roots, "account");
    expect([...found.hits]).toEqual(["j"]);
    expect([...found.open].sort()).toEqual(["f", "k"]);
  });
});
