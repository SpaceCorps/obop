// The org chart of Employees (`#/hr/org_chart`): who reports to whom, as a tree on the employee's
// manager (`reports_to_id`), with each person's photo or initials, job and department. It reads
// with `hr_employee.search`, the call an agent makes; a person opens their employee record.
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Avatar, Button, Icon, fmt, t } from "@obop/ui";
import { LOAD_LIMIT, search } from "../../api.ts";
import { blobUrl, thumbWidth } from "../../blobs.ts";
import type { App } from "../../Chrome.tsx";
import { useData } from "../../data.tsx";
import { href } from "../../route.ts";
import { serverMessage } from "../../serverMessage.ts";
import { EMPLOYEE } from "./links.ts";
import {
  narrowed,
  openToDepth,
  orgTree,
  pathsTo,
  treeStep,
  visibleRows,
  type OrgNode,
} from "./orgTree.ts";
import "./hr.css";

const DEPARTMENT = "hr_department";

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string } // the server's text, worded where it is drawn (D9)
  | { kind: "ready"; roots: OrgNode[]; total: number };

/** The tint of a person without a photo: the same as on their card in the Employees gallery. */
function tint(name: string): number {
  let h = 0;
  for (const c of name) h = (h * 31 + c.codePointAt(0)!) >>> 0;
  return (h % 6) + 1;
}

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
  return (
    <span className={`org-face t-${tint(name)}`}>
      <Avatar name={name} src={src} size={size} />
    </span>
  );
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
      if ("error" in people) return setState({ kind: "error", message: people.error });
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
  const searching = q.trim() !== "";
  // While a search is typed the chart shows only the matches and their managers, unfolded.
  const roots = useMemo(() => {
    if (state.kind !== "ready") return [];
    return searching ? narrowed(state.roots, new Set([...found.hits, ...found.open])) : state.roots;
  }, [state, searching, found]);
  const isOpen = (id: string) => (searching ? found.open.has(id) : open.has(id));
  const branches = useMemo(
    () => (state.kind === "ready" ? openToDepth(state.roots, Infinity) : new Set<string>()),
    [state],
  );
  const allOpen = [...branches].every((id) => open.has(id));
  const toggle = (id: string) =>
    setOpen((was) => {
      const next = new Set(was);
      if (isOpen(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const foldAll = () => setOpen(allOpen ? new Set() : new Set(branches));

  // The keyboard (the tree pattern): one Tab stop for the chart, the person who has it moves with
  // the arrow keys, Right and Left unfold and fold, Enter opens the person.
  const [focusId, setFocusId] = useState("");
  const treeRef = useRef<HTMLUListElement>(null);
  const rows = visibleRows(roots, isOpen);
  const current = rows.some((r) => r.id === focusId) ? focusId : (rows[0]?.id ?? "");
  const personHref = (id: string) => href({ app: app.id, model: EMPLOYEE, id });
  const onTreeKey = (e: KeyboardEvent<HTMLUListElement>) => {
    if ((e.target as HTMLElement).getAttribute("role") !== "treeitem") return;
    const step = treeStep(rows, current, e.key, isOpen);
    if (!step) return;
    e.preventDefault();
    if (step.open && !searching) setOpen((was) => new Set(was).add(step.open!));
    if (step.close && !searching)
      setOpen((was) => {
        const next = new Set(was);
        next.delete(step.close!);
        return next;
      });
    if (step.activate) location.hash = personHref(step.activate);
    if (step.focus) {
      setFocusId(step.focus);
      treeRef.current
        ?.querySelector<HTMLElement>(`[data-person="${CSS.escape(step.focus)}"]`)
        ?.focus();
    }
  };

  const node = (n: OrgNode, level: number) => {
    const expanded = n.children.length > 0 && isOpen(n.id);
    return (
      <li
        key={n.id}
        role="treeitem"
        data-person={n.id}
        tabIndex={n.id === current ? 0 : -1}
        aria-labelledby={`org-name-${n.id}`}
        aria-level={level}
        aria-expanded={n.children.length ? expanded : undefined}
        aria-selected={false}
        onFocus={(e) => e.target === e.currentTarget && setFocusId(n.id)}
      >
        <div className={`org-node${found.hits.has(n.id) ? " hit" : ""}`}>
          <a className="org-person" tabIndex={-1} href={personHref(n.id)}>
            <Face name={n.name} photo={n.photo} size={44} />
            <span className="org-text">
              <span className="org-name" id={`org-name-${n.id}`}>
                {n.name}
              </span>
              {(n.job || n.department) && (
                <span className="org-meta">
                  {[n.job, n.department].filter(Boolean).join(" · ")}
                </span>
              )}
            </span>
          </a>
          {n.children.length > 0 && !searching && (
            <button
              type="button"
              tabIndex={-1}
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
            {!searching && branches.size > 0 && (
              <Button
                variant="ghost"
                icon={allOpen ? "chevron-up" : "chevron-down"}
                onClick={foldAll}
              >
                {allOpen ? t("Collapse all") : t("Expand all")}
              </Button>
            )}
          </div>
        )}

        {state.kind === "loading" ? (
          <div className="org-wait" aria-busy="true" aria-label={t("Loading")}>
            {[0, 1, 2].map((i) => (
              <span key={i} className="skel" />
            ))}
          </div>
        ) : state.kind === "error" ? (
          <p className="hint">{serverMessage(state.message)}</p>
        ) : state.total === 0 ? (
          <p className="ov-calm">
            <Icon name="info" size={18} />
            {t("No employees yet. Add the first one, then set each person's manager.")}
          </p>
        ) : (
          <>
            <p className="org-found" role="status">
              {!searching
                ? ""
                : found.hits.size === 0
                  ? t("Nobody matches “{q}”.", { q: q.trim() })
                  : found.hits.size === 1
                    ? t("1 person matches, shown with their managers.")
                    : t("{count} people match, shown with their managers.", {
                        count: fmt.integer(found.hits.size),
                      })}
            </p>
            {roots.length > 0 && (
              <ul
                ref={treeRef}
                role="tree"
                className="org-tree"
                aria-label={t("Org chart")}
                onKeyDown={onTreeKey}
              >
                {roots.map((r) => node(r, 1))}
              </ul>
            )}
          </>
        )}
      </div>
    </section>
  );
}
