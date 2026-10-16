import { useCallback, useEffect, useRef, useState } from "react";

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
};

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
  };
}

export function href(r: Partial<Route>): string {
  const q = new URLSearchParams();
  if (r.view) q.set("view", r.view);
  if (r.id) q.set("id", r.id);
  if (r.tab) q.set("tab", r.tab);
  if (r.st) q.set("st", r.st);
  const qs = q.toString();
  return `#/${[r.app, r.model].filter(Boolean).join("/")}${qs ? `?${qs}` : ""}`;
}

/** Unsaved work registers here; leaving the page asks first. */
export const nav: {
  dirty: (() => boolean) | null;
  ask: ((message: string) => Promise<boolean>) | null;
} = { dirty: null, ask: null };

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
      if (nav.dirty?.() && nav.ask) {
        const target = location.hash;
        // Put the old address back until the person decides.
        history.replaceState(null, "", last.current || "#/");
        void nav.ask("You have unsaved changes. Leave and discard them?").then((leave) => {
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
  const go = useCallback<Go>((r, opts) => {
    const h = href(r);
    const apply = () => {
      if (opts?.replace) history.replaceState(null, "", h);
      else if (h !== location.hash) history.pushState(null, "", h);
      last.current = h;
      setRoute(parse(h));
    };
    if (nav.dirty?.() && nav.ask) {
      void nav.ask("You have unsaved changes. Leave and discard them?").then((leave) => {
        if (!leave) return;
        apply();
      });
    } else apply();
  }, []);
  return [route, go];
}
