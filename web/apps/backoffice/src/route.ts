import { useCallback, useEffect, useState } from "react";

/** The URL holds where you are, so links and the back button work (`#/crm/lead?view=kanban&id=…`). */
export type Route = { app: string; model: string; view: string; id: string; tab: string };

export function parse(hash: string): Route {
  const [path, query = ""] = hash.replace(/^#\/?/, "").split("?");
  const [app = "", model = ""] = path.split("/");
  const q = new URLSearchParams(query);
  return { app, model, view: q.get("view") ?? "", id: q.get("id") ?? "", tab: q.get("tab") ?? "" };
}

export function href(r: Partial<Route>): string {
  const q = new URLSearchParams();
  if (r.view) q.set("view", r.view);
  if (r.id) q.set("id", r.id);
  if (r.tab) q.set("tab", r.tab);
  const qs = q.toString();
  return `#/${[r.app, r.model].filter(Boolean).join("/")}${qs ? `?${qs}` : ""}`;
}

export function useRoute(): [Route, (r: Partial<Route>) => void] {
  const [route, setRoute] = useState(() => parse(location.hash));
  useEffect(() => {
    const on = () => setRoute(parse(location.hash));
    addEventListener("hashchange", on);
    return () => removeEventListener("hashchange", on);
  }, []);
  const go = useCallback((r: Partial<Route>) => {
    location.hash = href(r);
  }, []);
  return [route, go];
}
