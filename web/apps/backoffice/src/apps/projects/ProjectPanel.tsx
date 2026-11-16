// The top of a project (plan 3.5, Projects): how far it is (the share of tasks done, the time
// estimated, the deadline), its milestones as a strip in date order, and its open tasks as a
// checklist. Ticking a task runs the action that moves it to done from where it is
// (`projects_task.complete` from doing, `complete_now` from todo); "Add a task" runs
// `projects_task.create` with the project filled in. Nothing here is not a command (OR-4).
import { useEffect, useState, type FormEvent } from "react";
import { fmt, t } from "@obop/ui";
import { search, type Command, type FieldMeta, type ModelMeta, type Row } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData } from "../../data.tsx";
import { getTime } from "../../api.ts";
import { todayIn } from "../../filters.ts";
import { durationText } from "../../inputs/display.ts";
import { PercentBar } from "../../inputs/UnitInputs.tsx";
import { encodeState } from "../../query.ts";
import { serverMessage } from "../../serverMessage.ts";
import { Badge, Icon } from "../../ui.tsx";
import "./projects.css";

export const PROJECT = "projects_project";
export const TASK = "projects_task";
export const MILESTONE = "projects_milestone";

const OPEN_SHOWN = 8;
const isOpenTask = (r: Row) => r.state !== "done" && r.state !== "cancelled";
const shortDay = (day: unknown) =>
  typeof day === "string" && day
    ? fmt.dateOf(new Date(`${day.slice(0, 10)}T12:00:00Z`), {
        day: "numeric",
        month: "short",
        timeZone: "UTC",
      })
    : "";

/** The action that moves a task in `from` to done, if this person may run one. */
export function doneActionOf(task: ModelMeta, from: string): Command | undefined {
  return task.actions.find((a) => a.field === "state" && a.from === from && a.to === "done");
}

/** The records of `model` that belong to the project, asked again after every write. */
function useOfProject(model: string, project: string, enabled: boolean) {
  const d = useData();
  const [rows, setRows] = useState<Row[] | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    void search(d.token, model, {
      filter: [{ field: "project_id", op: "eq", value: project }],
      order: [{ field: "due_on", dir: "asc" }],
      limit: 500,
    }).then((r) => {
      if (live) setRows("error" in r ? [] : r.rows);
    });
    return () => {
      live = false;
    };
  }, [d.token, d.mutations, model, project, enabled]);
  return rows;
}

export function ProjectPanel({ row, busy }: RecordExtProps) {
  const d = useData();
  const task = d.models.get(TASK)?.model;
  const milestone = d.models.get(MILESTONE)?.model;
  const tasks = useOfProject(TASK, row.id, !!task);
  const milestones = useOfProject(MILESTONE, row.id, !!milestone);
  const [title, setTitle] = useState("");
  const [working, setWorking] = useState("");
  const today = todayIn(getTime().zone);

  const all = (tasks ?? []).filter((r) => r.state !== "cancelled");
  const done = all.filter((r) => r.state === "done").length;
  const open = (tasks ?? []).filter(isOpenTask);
  // The server's formula (done over not cancelled), from the tasks shown, so a tick moves it at
  // once; the stored value until they have loaded.
  const progress =
    tasks && all.length ? Math.floor((done * 100) / all.length) : Number(row.progress ?? 0);
  const estimate = Number(row.estimate_minutes ?? 0);
  const deadline = typeof row.deadline === "string" ? row.deadline : "";
  const late = !!deadline && deadline < today && row.state !== "done";
  const stateOf = (m: ModelMeta | undefined): FieldMeta | undefined =>
    m?.fields.find((f) => f.name === "state");
  const st = encodeState({ facets: [{ op: "is", f: "project_id", v: [row.id] }], group: [] });
  const allTasks = d.linkTo(TASK, { st });

  const tick = async (r: Row) => {
    const a = task && doneActionOf(task, String(r.state ?? ""));
    if (!a) return;
    setWorking(r.id);
    const out = await d.run(a.id, { id: r.id });
    setWorking("");
    if (out.kind === "error") d.notify(serverMessage(out.message), "error");
    else if (out.kind === "approval")
      d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
  };

  const add = async (e: FormEvent) => {
    e.preventDefault();
    const text = title.trim();
    if (!text) return;
    setWorking("new");
    const out = await d.run(`${TASK}.create`, { title: text, project_id: row.id });
    setWorking("");
    if (out.kind === "error") d.notify(serverMessage(out.message), "error");
    else setTitle("");
  };

  const shownMilestones = milestones ?? [];
  return (
    <section className="prj-panel" aria-label={t("Project progress")}>
      <div className="prj-progress">
        <div className="prj-progress-head">
          <span className="prj-pct num">{fmt.percent(progress / 100, 0)}</span>
          <span className="prj-progress-text">
            {all.length
              ? t("{done} of {count} tasks done", { done: String(done), count: String(all.length) })
              : t("No tasks yet")}
          </span>
        </div>
        <PercentBar share={Math.max(0, Math.min(1, progress / 100))} />
        <dl className="prj-facts">
          {estimate > 0 && (
            <div>
              <dt>{t("Estimated")}</dt>
              <dd className="num">{durationText(estimate)}</dd>
            </div>
          )}
          {deadline && (
            <div className={late ? "late" : undefined}>
              <dt>{t("Deadline")}</dt>
              <dd>
                {late && <Icon name="warning" size={14} />}
                {shortDay(deadline)}
              </dd>
            </div>
          )}
          {open.length > 0 && (
            <div>
              <dt>{t("Open tasks")}</dt>
              <dd className="num">{fmt.integer(open.length)}</dd>
            </div>
          )}
        </dl>
      </div>

      {milestone && shownMilestones.length > 0 && (
        <div className="prj-block">
          <h3 className="prj-h">{t("Milestones")}</h3>
          <ol className="prj-strip">
            {shownMilestones.map((m) => {
              const href = d.linkTo(MILESTONE, { id: m.id });
              const overdue =
                m.state === "pending" && typeof m.due_on === "string" && m.due_on < today;
              return (
                <li key={m.id} className={`prj-stone s-${String(m.state ?? "")}`}>
                  <span className="prj-dot" aria-hidden="true" />
                  <a className="prj-stone-name" href={href ?? undefined}>
                    {String(m.name ?? "")}
                  </a>
                  <span className={`prj-stone-date${overdue ? " late" : ""}`}>
                    {shortDay(m.due_on)}
                  </span>
                  <Badge field={stateOf(milestone)} value={String(m.state ?? "")} />
                </li>
              );
            })}
          </ol>
        </div>
      )}

      {task && (
        <div className="prj-block">
          <div className="prj-block-head">
            <h3 className="prj-h">{t("Open tasks")}</h3>
            {allTasks && (
              <a href={allTasks} className="prj-all">
                {t("All tasks")}
                <Icon name="chevron-right" size={14} />
              </a>
            )}
          </div>
          {tasks && open.length === 0 && (
            <p className="hint prj-none">
              {all.length ? t("Every task is done.") : t("Add the first task below.")}
            </p>
          )}
          {open.length > 0 && (
            <ul className="prj-tasks">
              {open.slice(0, OPEN_SHOWN).map((r) => {
                const a = doneActionOf(task, String(r.state ?? ""));
                const name = String(r.title ?? "");
                const due = typeof r.due_on === "string" ? r.due_on : "";
                return (
                  <li key={r.id} className="prj-task">
                    <input
                      type="checkbox"
                      className="prj-check"
                      aria-label={t("Mark {task} as done", { task: name })}
                      checked={false}
                      disabled={!a || busy || working === r.id}
                      onChange={() => void tick(r)}
                    />
                    <a className="prj-task-name" href={d.linkTo(TASK, { id: r.id }) ?? undefined}>
                      {name}
                    </a>
                    <span className="prj-task-meta">
                      {typeof r.assignee === "string" && r.assignee && (
                        <span className="prj-who">{r.assignee}</span>
                      )}
                      {due && (
                        <span className={`prj-due${due < today ? " late" : ""}`}>
                          {shortDay(due)}
                        </span>
                      )}
                      <Badge field={stateOf(task)} value={String(r.state ?? "")} />
                    </span>
                  </li>
                );
              })}
              {open.length > OPEN_SHOWN && (
                <li className="hint prj-more">
                  {t("and {count} more", { count: String(open.length - OPEN_SHOWN) })}
                </li>
              )}
            </ul>
          )}
          {task.can_create && (
            <form className="prj-add" onSubmit={(e) => void add(e)}>
              <Icon name="plus" size={16} />
              <input
                type="text"
                value={title}
                aria-label={t("New task title")}
                placeholder={t("Add a task…")}
                onChange={(e) => setTitle(e.target.value)}
                disabled={working === "new"}
              />
              {title.trim() && (
                <button type="submit" className="btn small primary" disabled={working === "new"}>
                  {t("Add")}
                </button>
              )}
            </form>
          )}
        </div>
      )}
    </section>
  );
}
