// orgTree.ts: the org chart's tree on `reports_to_id`.
import { describe, expect, it } from "vite-plus/test";
import type { Row } from "../../api.ts";
import { narrowed, openToDepth, orgTree, pathsTo, treeStep, visibleRows } from "./orgTree.ts";

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

  it("narrows the chart to the hits and their managers", () => {
    const roots = orgTree(
      [
        p("k", "Katrin"),
        p("f", "Frank", "k"),
        p("j", "Julia", "f", "Accountant"),
        p("a", "Anna", "k", "Driver"),
      ],
      dept,
    );
    const found = pathsTo(roots, "account");
    const keep = new Set([...found.hits, ...found.open]);
    const shown = narrowed(roots, keep);
    expect(shown.map((r) => r.name)).toEqual(["Katrin"]);
    expect(shown[0]!.children.map((c) => c.name)).toEqual(["Frank"]);
    expect(shown[0]!.children[0]!.children.map((c) => c.name)).toEqual(["Julia"]);
    // The department is searched too.
    expect(pathsTo(roots, "sales").hits.size).toBe(4);
  });
});

describe("the keyboard on the chart", () => {
  // Katrin > (Anna, Frank > Julia)
  const roots = orgTree(
    [p("k", "Katrin"), p("f", "Frank", "k"), p("a", "Anna", "k"), p("j", "Julia", "f")],
    dept,
  );
  const open = new Set(["k"]);
  const isOpen = (id: string) => open.has(id);

  it("lists the people shown, top to bottom, a branch's reports only while unfolded", () => {
    expect(visibleRows(roots, isOpen).map((r) => r.id)).toEqual(["k", "a", "f"]);
    expect(visibleRows(roots, (id) => id === "k" || id === "f").map((r) => r.id)).toEqual([
      "k",
      "a",
      "f",
      "j",
    ]);
  });

  it("moves, unfolds, folds and opens like a tree", () => {
    const rows = visibleRows(roots, isOpen);
    expect(treeStep(rows, "k", "ArrowDown", isOpen)).toEqual({ focus: "a" });
    expect(treeStep(rows, "a", "ArrowUp", isOpen)).toEqual({ focus: "k" });
    expect(treeStep(rows, "a", "End", isOpen)).toEqual({ focus: "f" });
    expect(treeStep(rows, "f", "Home", isOpen)).toEqual({ focus: "k" });
    // Right on a folded branch unfolds it; on an unfolded one it moves to the first report.
    expect(treeStep(rows, "f", "ArrowRight", isOpen)).toEqual({ open: "f" });
    expect(treeStep(rows, "k", "ArrowRight", isOpen)).toEqual({ focus: "a" });
    // Left on an unfolded branch folds it; on anyone else it moves to the manager.
    expect(treeStep(rows, "k", "ArrowLeft", isOpen)).toEqual({ close: "k" });
    expect(treeStep(rows, "a", "ArrowLeft", isOpen)).toEqual({ focus: "k" });
    expect(treeStep(rows, "a", "Enter", isOpen)).toEqual({ activate: "a" });
    expect(treeStep(rows, "a", "x", isOpen)).toBeNull();
  });
});
