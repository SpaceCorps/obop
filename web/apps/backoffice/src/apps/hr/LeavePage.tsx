// Time off: the generic page, except that a new leave request starts on the employee record of the
// person signed in (`hr_employee.user`), so an employee filing their own leave on a phone does not
// have to find themselves in a list of names. The server decides whose request it is
// (`requested_by` is filled from the employee's user); this only saves the search.
import { useEffect, useRef, useState } from "react";
import { search } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { ModelPageProps } from "../../extensions.ts";
import { ModelPage } from "../../ModelPage.tsx";
import { EMPLOYEE } from "./links.ts";

const cache = new Map<string, Promise<string>>();

function signedIn(): string {
  try {
    return sessionStorage.getItem("obop-user") ?? "";
  } catch {
    return "";
  }
}

/**
 * The id of the one employee whose `user` is the person signed in ("" when there is none, several,
 * or the reader cannot see the field). Asked once per session and user.
 */
export function myEmployee(token: string): Promise<string> {
  const user = signedIn();
  if (!user || !token) return Promise.resolve("");
  const key = `${token.slice(-8)}:${user}`;
  let p = cache.get(key);
  if (!p) {
    p = search(token, EMPLOYEE, {
      filter: [{ field: "user", op: "eq", value: user }],
      limit: 2,
    }).then((r) => ("error" in r || r.rows.length !== 1 ? "" : r.rows[0]!.id));
    cache.set(key, p);
  }
  return p;
}

/** Whether the leave model has an employee to start on (a package before 0.4.0 has no `user`). */
const hasUserField = (p: ModelPageProps) =>
  p.app.module.models.some((m) => m.name === EMPLOYEE && m.fields.some((f) => f.name === "user"));

export function LeavePage(p: ModelPageProps) {
  const { token } = useData();
  const [me, setMe] = useState("");
  const linked = hasUserField(p);
  useEffect(() => {
    let live = true;
    if (linked) void myEmployee(token).then((id) => live && setMe(id));
    return () => {
      live = false;
    };
  }, [token, linked]);
  // The generic page reads its starting values when it opens on a new record, so the page is drawn
  // afresh when the route turns to one: it then starts on the person's own employee record.
  const [gen, setGen] = useState(0);
  const was = useRef(p.route.id);
  useEffect(() => {
    if (p.route.id === "new" && was.current !== "new" && me) setGen((g) => g + 1);
    was.current = p.route.id;
  }, [p.route.id, me]);
  return <ModelPage key={gen} {...p} takeNewDefaults={() => (me ? { employee_id: me } : null)} />;
}
