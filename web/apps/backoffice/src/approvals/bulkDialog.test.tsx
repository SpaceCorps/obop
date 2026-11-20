// @vitest-environment jsdom
// The result list of a bulk act: a link to a waiting plan has a name that includes its row's label,
// so a screen reader's list of links does not read "waiting for approval" over and over (D97, rule 10).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { BulkDialog } from "../BulkDialog.tsx";
import type { Outcome } from "../bulk.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

it("in the result list each link's name includes its row's label", async () => {
  const rows = [{ id: "a" }, { id: "b" }, { id: "c" }];
  const outcomes: Record<string, Outcome> = {
    a: { id: "a", label: "Acme", status: "approval", planId: "plan-a" },
    b: { id: "b", label: "Beta", status: "done" },
    c: { id: "c", label: "Cato", status: "approval", planId: "plan-c" },
  };
  await act(async () => {
    root.render(
      <BulkDialog
        title="Delete leads"
        question={(n) => `Delete ${n} leads?`}
        confirmLabel={() => "Delete"}
        total={3}
        collect={async () => ({ rows, total: 3, truncated: false, cancelled: false })}
        apply={async (row) => outcomes[row.id]!}
        labelOf={(row) => outcomes[row.id]!.label}
        noun={() => "leads"}
        onClose={() => {}}
        onDone={() => {}}
      />,
    );
  });
  const go = [...document.querySelectorAll("button")].find((b) => b.textContent === "Delete")!;
  await act(async () => go.click());
  await act(async () => new Promise((r) => setTimeout(r, 50)));
  const links = [...document.querySelectorAll<HTMLAnchorElement>(".results a")];
  expect(links.map((a) => a.getAttribute("aria-label"))).toEqual([
    "Acme, waiting for approval",
    "Cato, waiting for approval",
  ]);
  expect(links.map((a) => a.textContent)).toEqual(["waiting for approval", "waiting for approval"]);
});
