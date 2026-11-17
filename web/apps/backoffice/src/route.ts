import { useCallback, useEffect, useRef, useState } from "react";
import { t } from "@obop/ui";
import { heldLeaveQuestions, type LeaveQuestion } from "./leaveGuard.ts";

/**
 * The URL holds where you are, so links and the back button work:
 * `#/crm/lead?view=kanban&id=…&st={…}`. `st` is the encoded search state (facets, grouping, sort).
 */
export type Route = {
  app: string;
  model: string;
  view: string;
  id: string;
  tab: string;
  st: string;
  /** The page of a list, 1-based; empty is the first. */
  page: string;
  /** A section inside a Customize tab, for example a heading of the module guide. */
  section: string;
  /** "1": the open record fills the page instead of the side panel (`&full=1`); empty: the panel. */
  full: string;
  /** A field of the open record to bring into view and focus (`&field=tax_id`, Q19). */
  field: string;
};

/** The route name of an app's generated overview page: `#/<app>/overview`. */
export const OVERVIEW = "overview";

/** Whether the route shows an app's overview page. */
export const isOverview = (r: Pick<Route, "app" | "model">): boolean =>
  !!r.app && r.app !== "customize" && r.model === OVERVIEW;

/** The address of an app's overview page. */
export const overviewHref = (app: string): string => href({ app, model: OVERVIEW });

/** Whether the open record is shown as a full page. */
export const isFull = (r: Pick<Route, "full">): boolean => r.full === "1";

/** The same route with the record as a full page (`on`) or in the side panel. */
export const withFull = (r: Partial<Route>, on: boolean): Partial<Route> => ({
  ...r,
  full: on ? "1" : "",
});

export function parse(hash: string): Route {
  const [path = "", query = ""] = hash.replace(/^#\/?/, "").split("?");
  const [app = "", model = ""] = path.split("/");
  const q = new URLSearchParams(query);
  return {
    app,
    model,
    view: q.get("view") ?? "",
    id: q.get("id") ?? "",
    tab: q.get("tab") ?? "",
    st: q.get("st") ?? "",
    page: q.get("page") ?? "",
    section: q.get("section") ?? "",
    full: q.get("full") === "1" ? "1" : "",
    field: /^[a-z][a-z0-9_]*$/.test(q.get("field") ?? "") ? q.get("field")! : "",
  };
}

/** Whether two addresses are the same place, however they are written (`#`, `#/`, an empty view). */
function sameRoute(a: string, b: string): boolean {
  const x = parse(a);
  const y = parse(b);
  return (Object.keys(x) as (keyof Route)[]).every((k) => x[k] === y[k]);
}

export function href(r: Partial<Route>): string {
  const q = new URLSearchParams();
  if (r.view) q.set("view", r.view);
  if (r.id) q.set("id", r.id);
  if (r.tab) q.set("tab", r.tab);
  if (r.st) q.set("st", r.st);
  if (r.page && r.page !== "1") q.set("page", r.page);
  if (r.section) q.set("section", r.section);
  if (r.full === "1") q.set("full", "1");
  if (r.field) q.set("field", r.field);
  const qs = q.toString();
  return `#/${[r.app, r.model].filter(Boolean).join("/")}${qs ? `?${qs}` : ""}`;
}

/**
 * Unsaved work registers here; leaving the page asks first. So does anything that is lost for good
 * when the person leaves (`leaveGuard.ts`, a signing link that was not copied).
 *
 * `saving` is the save that is on its way, if any: a promise that settles when the save is over,
 * whatever came of it. Its changes are still `dirty`, yet they are being saved, so "leave and
 * discard them?" would be a question about nothing (Create, then Escape before the server has
 * answered). Leaving waits for the save instead, and what the save left decides what is asked.
 */
export const nav: {
  dirty: (() => boolean) | null;
  saving: (() => Promise<void> | null) | null;
  ask: ((message: string, leave: string, stay: string) => Promise<boolean>) | null;
} = { dirty: null, saving: null, ask: null };

/**
 * What to ask before the address changes, in order; nothing to ask means go ahead. Unsaved changes
 * and things that are lost for good (`leaveGuard.ts`, one question for each) can all be true. When
 * the first hold has words for a person who also has unsaved changes, one question names both;
 * otherwise the unsaved changes are asked about first and every hold after them, so none is dropped.
 */
function leaveQuestions(): LeaveQuestion[] {
  const held = heldLeaveQuestions();
  if (!nav.dirty?.()) return held;
  const [first, ...rest] = held;
  if (first?.whenUnsaved)
    return [{ message: first.whenUnsaved, leave: t("Leave anyway"), stay: t("Stay") }, ...rest];
  return [
    {
      message: t("You have unsaved changes. Leave and discard them?"),
      leave: t("Leave and discard"),
      stay: t("Keep editing"),
    },
    ...held,
  ];
}

/** Ask each question in turn. The page is left only when the person says leave to every one. */
async function askAll(questions: LeaveQuestion[]): Promise<boolean> {
  for (const q of questions) if (!(await nav.ask?.(q.message, q.leave, q.stay))) return false;
  return true;
}

export type Go = (r: Partial<Route>, opts?: { replace?: boolean }) => void;

export function useRoute(): [Route, Go] {
  const [route, setRoute] = useState(() => parse(location.hash));
  const last = useRef(location.hash);

  useEffect(() => {
    let skip = false;
    const on = () => {
      if (skip) {
        skip = false;
        last.current = location.hash;
        setRoute(parse(location.hash));
        return;
      }
      const saving = nav.saving?.();
      if (saving) {
        const target = location.hash;
        // Put the old address back until the save is over, then go there again: asked then, if
        // the save was refused and the changes are unsaved after all.
        history.replaceState(null, "", last.current || "#/");
        void saving.then(() => {
          if (location.hash !== target) location.hash = target;
        });
        return;
      }
      const questions = nav.ask ? leaveQuestions() : [];
      if (questions.length) {
        const target = location.hash;
        // Put the old address back until the person decides.
        history.replaceState(null, "", last.current || "#/");
        void askAll(questions).then((leave) => {
          if (!leave) return;
          skip = true;
          location.hash = target;
        });
        return;
      }
      last.current = location.hash;
      setRoute(parse(location.hash));
    };
    addEventListener("hashchange", on);
    return () => removeEventListener("hashchange", on);
  }, []);

  // Navigating from code updates the route at once (no wait for the hashchange event).
  const go = useCallback<Go>(function go(r, opts) {
    const h = href(r);
    const apply = () => {
      if (opts?.replace) history.replaceState(null, "", h);
      else if (h !== location.hash) history.pushState(null, "", h);
      last.current = h;
      setRoute(parse(h));
    };
    // Going to the address that is already shown (saving a record goes to its own address) leaves
    // nothing behind, so nothing is asked.
    const leaving = !sameRoute(h, location.hash);
    // A save is on its way: go when it is over (see `nav.saving`).
    const saving = leaving ? nav.saving?.() : null;
    if (saving) {
      void saving.then(() => go(r, opts));
      return;
    }
    const questions = nav.ask && leaving ? leaveQuestions() : [];
    if (questions.length) {
      void askAll(questions).then((leave) => {
        if (leave) apply();
      });
    } else apply();
  }, []);
  return [route, go];
}
