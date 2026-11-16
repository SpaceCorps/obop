// The org chart of Employees (`#/hr/org_chart`): who reports to whom, as a tree on the employee's
// manager (`reports_to_id`), with each person's photo or initials, job and department. It reads
// with `hr_employee.search`, the call an agent makes; a person opens their employee record.
import { useEffect, useMemo, useState } from "react";
import { Avatar, Button, Icon, fmt, t } from "@obop/ui";
import { LOAD_LIMIT, search } from "../../api.ts";
import { blobUrl, thumbWidth } from "../../blobs.ts";
import type { App } from "../../Chrome.tsx";
import { useData } from "../../data.tsx";
import { href } from "../../route.ts";
import { serverMessage } from "../../serverMessage.ts";
import { EMPLOYEE } from "./links.ts";
import { openToDepth, orgTree, pathsTo, type OrgNode } from "./orgTree.ts";
import "./hr.css";

const DEPARTMENT = "hr_department";

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | { kind: "ready"; roots: OrgNode[]; total: number };

/** A photo from the file store, or the person's initials while it loads or when there is none. */
function Face({ name, photo, size }: { name: string; photo: string; size: number }) {
  const d = useData();
  const [src, setSrc] = useState<string | undefined>();
  useEffect(() => {
    let live = true;
    setSrc(undefined);
    if (photo)
      void blobUrl(d.token, photo, thumbWidth(size)).then((u) => live && setSrc(u ?? undefined));
    return () => {
      live = false;
    };
  }, [d.token, photo, size]);
  return <Avatar name={name} src={src} size={size} />;
}

export function OrgChart({ app }: { app: App }) {
  const d = useData();
  const [state, setState] = useState<State>({ kind: "loading" });
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [q, setQ] = useState("");
  const employee = app.module.models.find((m) => m.name === EMPLOYEE);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [people, depts] = await Promise.all([
        search(d.token, EMPLOYEE, {
          filter: [{ field: "state", op: "ne", value: "offboarded" }],
          limit: LOAD_LIMIT,
        }),
        search(d.token, DEPARTMENT, { limit: LOAD_LIMIT }),
      ]);
      if (!live) return;
      if ("error" in people)
        return setState({ kind: "error", message: serverMessage(people.error) });
      const names = new Map(
        ("error" in depts ? [] : depts.rows).map((r) => [r.id, d.recordLabel(DEPARTMENT, r)]),
      );
      const roots = orgTree(people.rows, (id) => names.get(id) ?? "");
      setState({ kind: "ready", roots, total: people.rows.length });
      setOpen((was) => (was.size ? was : openToDepth(roots, 2)));
    })();
    return () => {
      live = false;
    };
  }, [d.token, d.mutations]);

  const found = useMemo(
    () =>
      state.kind === "ready"
        ? pathsTo(state.roots, q)
        : { open: new Set<string>(), hits: new Set<string>() },
    [state, q],
  );
  const isOpen = (id: string) => open.has(id) || found.open.has(id);
  const toggle = (id: string) =>
    setOpen((was) => {
      const next = new Set(was);
      if (isOpen(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const all = () => {
    if (state.kind !== "ready") return;
    setOpen(openToDepth(state.roots, Infinity));
  };

  const node = (n: OrgNode, level: number) => {
    const expanded = n.children.length > 0 && isOpen(n.id);
    return (
      <li
        key={n.id}
        role="treeitem"
        aria-level={level}
        aria-expanded={n.children.length ? expanded : undefined}
        aria-selected={false}
      >
        <div className={`org-node${found.hits.has(n.id) ? " hit" : ""}`}>
          <a className="org-person" href={href({ app: app.id, model: EMPLOYEE, id: n.id })}>
            <Face name={n.name} photo={n.photo} size={44} />
            <span className="org-text">
              <span className="org-name">{n.name}</span>
              {(n.job || n.department) && (
                <span className="org-meta">
                  {[n.job, n.department].filter(Boolean).join(" · ")}
                </span>
              )}
            </span>
          </a>
          {n.children.length > 0 && (
            <button
              type="button"
              className="org-toggle"
              aria-expanded={expanded}
              aria-label={
                expanded
                  ? t("Hide the {count} people below {name}", {
                      count: fmt.integer(n.below),
                      name: n.name,
                    })
                  : t("Show the {count} people below {name}", {
                      count: fmt.integer(n.below),
                      name: n.name,
                    })
              }
              onClick={() => toggle(n.id)}
            >
              <Icon name="users" size={14} />
              <span>{fmt.integer(n.below)}</span>
              <Icon name={expanded ? "chevron-up" : "chevron-down"} size={14} />
            </button>
          )}
        </div>
        {expanded && (
          <ul role="group" className="org-group">
            {n.children.map((c) => node(c, level + 1))}
          </ul>
        )}
      </li>
    );
  };

  return (
    <section className="main panel overview hr-org" aria-label={t("Org chart")}>
      <div className="content">
        <header className="ov-head">
          <div className="ov-title">
            <h1>{t("Org chart")}</h1>
            <p className="hint">
              {state.kind === "ready"
                ? t("{count} people, by who they report to. Open a person to see their details.", {
                    count: fmt.integer(state.total),
                  })
                : t("Who reports to whom.")}
            </p>
          </div>
          {employee?.can_create && (
            <div className="ov-actions">
              <Button
                variant="primary"
                icon="plus"
                onClick={() => (location.hash = href({ app: app.id, model: EMPLOYEE, id: "new" }))}
              >
                {t("New employee")}
              </Button>
            </div>
          )}
        </header>

        {state.kind === "ready" && state.total > 0 && (
          <div className="org-tools">
            <label className="org-search">
              <Icon name="search" size={16} />
              <input
                type="search"
                value={q}
                placeholder={t("Find a person or a job")}
                aria-label={t("Find a person or a job")}
                onChange={(e) => setQ(e.target.value)}
              />
            </label>
            <Button variant="ghost" onClick={all}>
              {t("Show everyone")}
            </Button>
          </div>
        )}

        {state.kind === "loading" ? (
          <div className="org-wait" aria-busy="true" aria-label={t("Loading")}>
            {[0, 1, 2].map((i) => (
              <span key={i} className="skel" />
            ))}
          </div>
        ) : state.kind === "error" ? (
          <p className="hint">{state.message}</p>
        ) : state.total === 0 ? (
          <p className="ov-calm">
            <Icon name="info" size={18} />
            {t("No employees yet. Add the first one, then set each person's manager.")}
          </p>
        ) : (
          <>
            {q.trim() && found.hits.size === 0 && (
              <p className="hint org-none">{t("Nobody matches “{q}”.", { q: q.trim() })}</p>
            )}
            <ul role="tree" className="org-tree" aria-label={t("Org chart")}>
              {state.roots.map((r) => node(r, 1))}
            </ul>
          </>
        )}
      </div>
    </section>
  );
}
