// @vitest-environment jsdom
// The pieces of the Approvals page other screens use (stage P2): the address of the page and of one
// plan, the count hook and its badge, and the page's sentences built from a plan's data.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { parse } from "../route.ts";
import type { PlanCount, PlanRow } from "./planApi.ts";
import { reasonLength } from "./planApi.ts";
import { APPROVALS, approvalsHref, isApprovals, tabOf } from "./route.ts";
import { badgeText, plansChanged, showsApprovals, usePlanCount } from "./usePlanCount.ts";
import { approveLabel, decidedSentence, thingOf, whoOf, type Names } from "./words.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
});

describe("the address", () => {
  it("is #/approvals, its Decided list, or one plan", () => {
    expect(approvalsHref()).toBe("#/approvals");
    expect(approvalsHref({ tab: "decided" })).toBe("#/approvals?view=decided");
    expect(approvalsHref({ id: "01ABC" })).toBe("#/approvals?id=01ABC");
    const r = parse(approvalsHref({ id: "01ABC" }));
    expect([r.app, r.model, r.id, isApprovals(r), tabOf(r)]).toEqual([
      APPROVALS,
      "",
      "01ABC",
      true,
      "waiting",
    ]);
    expect(tabOf(parse("#/approvals?view=decided"))).toBe("decided");
    expect(isApprovals(parse("#/crm/lead"))).toBe(false);
  });
});

describe("the count", () => {
  it("shows the entry when the person can decide or something waits; 9+ above nine", () => {
    const c = (waiting: number, can_decide: boolean): PlanCount => ({
      waiting,
      mine_waiting: 0,
      can_decide,
    });
    expect(showsApprovals(null)).toBe(false);
    expect(showsApprovals(c(0, false))).toBe(false);
    expect(showsApprovals(c(0, true))).toBe(true);
    expect(showsApprovals(c(2, false))).toBe(true);
    expect([badgeText(1), badgeText(9), badgeText(10), badgeText(250)]).toEqual([
      "1",
      "9",
      "9+",
      "9+",
    ]);
  });

  describe("usePlanCount", () => {
    let root: Root;
    let container: HTMLDivElement;
    let reads: number;
    let shown: PlanCount | null;
    let hidden: boolean;
    function Probe() {
      shown = usePlanCount("person-token");
      return null;
    }
    beforeEach(() => {
      reads = 0;
      shown = null;
      hidden = false;
      vi.spyOn(document, "visibilityState", "get").mockImplementation(() =>
        hidden ? "hidden" : "visible",
      );
      vi.stubGlobal(
        "fetch",
        vi.fn(async (url: string) => {
          if (String(url) === "/api/commands")
            return new Response(JSON.stringify({ commands: [{ id: "plan.count" }] }), {
              headers: { "content-type": "application/json" },
            });
          expect(String(url)).toMatch(/^\/api\/plan\/count/);
          reads++;
          return new Response(
            JSON.stringify({ value: { waiting: reads, mine_waiting: 0, can_decide: true } }),
            { headers: { "content-type": "application/json" } },
          );
        }),
      );
      container = document.createElement("div");
      document.body.append(container);
      root = createRoot(container);
    });
    afterEach(async () => {
      await act(async () => root.unmount());
      container.remove();
      vi.unstubAllGlobals();
      vi.restoreAllMocks();
    });
    const settle = () => act(async () => new Promise((r) => setTimeout(r, 0)));

    it("reads on mount, after a decision and on focus, and not while the tab is hidden", async () => {
      await act(async () => root.render(<Probe />));
      await settle();
      expect(reads).toBe(1);
      expect(shown?.waiting).toBe(1);
      await act(async () => plansChanged());
      await settle();
      expect(reads).toBe(2);
      await act(async () => dispatchEvent(new Event("focus")));
      await settle();
      expect(reads).toBe(3);
      hidden = true;
      await act(async () => document.dispatchEvent(new Event("visibilitychange")));
      await act(async () => plansChanged());
      await settle();
      expect(reads, "a read while hidden").toBe(3);
      hidden = false;
      await act(async () => document.dispatchEvent(new Event("visibilitychange")));
      await settle();
      expect(reads).toBe(4);
    });
  });
});

describe("the words", () => {
  const names: Names = {
    model: (m) => (m === "invoice" ? "Invoice" : "Contact"),
    action: (c) => (c === "invoice.post" ? "Post" : "Archive"),
    fieldOf: (_m, f) => ({ title: f }),
  };
  const row = (over: Partial<PlanRow>): PlanRow => ({
    id: "01HZX9PLANIDXYZ",
    command: "invoice.update",
    model: "invoice",
    action: "update",
    kind: "update",
    record_id: "r1",
    record_title: "INV-0042",
    proposer: "ada/agent",
    owner: "ada",
    own_agent: false,
    status: "waiting",
    destructive: false,
    can_approve: true,
    can_reject: true,
    proposer_can_run: true,
    ...over,
  });

  it("the filled button names the act of each kind", () => {
    expect(approveLabel(row({ kind: "update" }), names)).toBe("Approve and save");
    expect(approveLabel(row({ kind: "create" }), names)).toBe("Approve and create");
    expect(approveLabel(row({ kind: "bulk_create" }), names)).toBe("Approve and create");
    expect(approveLabel(row({ kind: "delete" }), names)).toBe("Approve and delete");
    expect(approveLabel(row({ kind: "action", command: "invoice.post" }), names)).toBe(
      "Approve and post",
    );
    expect(approveLabel(row({ kind: "other" }), names)).toBe("Approve");
  });

  it("names the thing from meta labels and the record's title, never the plan id", () => {
    const kinds: PlanRow["kind"][] = [
      "update",
      "create",
      "bulk_create",
      "delete",
      "action",
      "other",
    ];
    for (const kind of kinds) {
      const r = row({ kind, command: kind === "action" ? "invoice.post" : `invoice.${kind}` });
      for (const s of [thingOf(r, names), approveLabel(r, names), whoOf(r)])
        expect(s, kind).not.toContain(r.id);
    }
    expect(thingOf(row({ kind: "action", command: "invoice.post" }), names)).toBe(
      "Post invoice “INV-0042”",
    );
    expect(thingOf(row({ kind: "create", record_id: null, record_title: null }), names)).toBe(
      "Create invoice",
    );
    // An action whose label names the model already does not name it twice.
    const approveInvoice: Names = { ...names, action: () => "Approve invoice" };
    expect(thingOf(row({ kind: "action", command: "invoice.approve" }), approveInvoice)).toBe(
      "Approve invoice “INV-0042”",
    );
    // A title without a record id is not shown (the record line is null).
    expect(thingOf(row({ record_id: null }), names)).toBe("Change invoice");
  });

  it("says who asked and what became of a decided plan", () => {
    expect(whoOf(row({ owner: "ada" }))).toBe("ada's agent");
    expect(whoOf(row({ own_agent: true }))).toBe("Your own agent");
    expect(whoOf(row({ owner: null }))).toBe("An agent");
    expect(decidedSentence({ status: "approved", decided_by: "bo", decided_at: null })).toBe(
      "bo approved this.",
    );
    expect(decidedSentence({ status: "expired" })).toBe(
      "This request expired. Nothing was changed. Ask the agent to propose it again.",
    );
  });

  it("counts a reason in characters, as the server does", () => {
    expect(reasonLength("  abc  ")).toBe(3);
    expect(reasonLength("😀😀")).toBe(2);
  });
});
