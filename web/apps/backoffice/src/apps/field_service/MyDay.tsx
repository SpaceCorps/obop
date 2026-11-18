// Field Service: "My day" (`#/field_service/my-day`), the technician's list for today on a phone or
// at a desk. The jobs of the employee linked to the signed-in user (`hr_employee.user`), scheduled
// today and not cancelled, in order of their start time; each card has the address with a map link,
// the contact's phone and the one next step of the job (On my way, Start work, Complete). It only
// calls commands: `field_service_job.search` and the job's actions `depart`, `start` and
// `complete`, the calls an agent or the command line makes for the same thing (OR-4).
import { useEffect, useState } from "react";
import { Badge, Button, EmptyState, Icon, Segmented, t } from "@obop/ui";
import { search, type Row } from "../../api.ts";
import type { App } from "../../Chrome.tsx";
import { useData } from "../../data.tsx";
import { formatDate, isoDate, optionLabel } from "../../format.ts";
import { notifySent } from "../../approvals/sent.ts";
import { actionTitle } from "../../labels.ts";
import { clockText, linkOf, shownNumber } from "../../inputs/display.ts";
import { serverMessage } from "../../serverMessage.ts";
import { myEmployee } from "../hr/LeavePage.tsx";
import { JOB, nextStep, needsSignature, siteText } from "./job.ts";
import "./field_service.css";

type Who = "mine" | "all";

const str = (v: unknown) => (typeof v === "string" ? v : "");

export function MyDay({ app }: { app: App }) {
  const d = useData();
  const model = d.models.get(JOB)?.model;
  const today = isoDate(new Date());
  const [me, setMe] = useState<string | null>(null);
  const [who, setWho] = useState<Who>("mine");
  const [jobs, setJobs] = useState<Row[] | null>(null);
  const [error, setError] = useState("");
  const [working, setWorking] = useState("");

  useEffect(() => {
    let live = true;
    void myEmployee(d.token).then((id) => live && setMe(id));
    return () => {
      live = false;
    };
  }, [d.token]);

  const mine = who === "mine" && !!me;
  useEffect(() => {
    if (me === null) return;
    let live = true;
    void search(d.token, JOB, {
      filter: [
        { field: "scheduled_on", op: "eq", value: today },
        { field: "state", op: "ne", value: "cancelled" },
        ...(mine ? [{ field: "technician_id", op: "eq" as const, value: me }] : []),
      ],
      order: [{ field: "scheduled_start_minute", dir: "asc" }],
      limit: 200,
    }).then((r) => {
      if (!live) return;
      if ("error" in r) {
        setError(r.error);
        setJobs([]);
      } else {
        setError("");
        setJobs(r.rows);
      }
    });
    return () => {
      live = false;
    };
  }, [d.token, today, me, mine, d.mutations]);

  const stateField = model?.fields.find((f) => f.name === "state");
  // The same name and the same question as the action has on the job and the board.
  const command = (action: string) => model?.actions.find((c) => c.id === `${JOB}.${action}`);
  const run = async (job: Row, action: string, done: string) => {
    const cmd = command(action);
    if (cmd?.ui?.confirm && !(await d.confirm(cmd.ui.confirm, actionTitle(cmd), t("Cancel"))))
      return;
    setWorking(job.id);
    try {
      const r = await d.run(`${JOB}.${action}`, { id: job.id });
      if (r.kind === "ok") d.notify(done, "ok");
      else if (r.kind === "approval") notifySent(d.notify, r.planId);
      else d.notify(serverMessage(r.message), "error");
    } finally {
      setWorking("");
    }
  };

  const open = (job: Row) => {
    const to = d.linkTo(JOB, { id: job.id });
    if (to) location.hash = to;
  };

  const doneCount = (jobs ?? []).filter((j) => j.state === "done").length;
  // The one primary button of the screen: the next step of the first job that is not done.
  const nextId = (jobs ?? []).find((j) => j.state !== "done")?.id;
  const allJobs = d.linkTo(JOB);

  return (
    <section className="main panel overview fs-day" aria-labelledby="fs-day-title">
      <div className="content">
        <header className="fs-day-head">
          <div>
            <h1 id="fs-day-title">{t("My day")}</h1>
            <p className="hint">
              {formatDate(today)}
              {jobs && jobs.length > 0 && (
                <>
                  {" · "}
                  {t("{done} of {count, plural, one {# job} other {# jobs}} done", {
                    done: doneCount,
                    count: jobs.length,
                  })}
                </>
              )}
            </p>
          </div>
          {me && (
            <Segmented<Who>
              label={t("Whose jobs")}
              value={who}
              onChange={setWho}
              options={[
                { value: "mine", label: t("Mine") },
                { value: "all", label: t("Everyone") },
              ]}
            />
          )}
        </header>
        {me === "" && (
          <p className="hint fs-day-note">
            <Icon name="info" size={14} />
            {t(
              "Your user is not linked to an employee, so this shows every job of today. An HR manager links it on the employee record.",
            )}
          </p>
        )}
        {error && <p className="hint">{serverMessage(error)}</p>}
        {jobs === null ? (
          <div className="fs-day-list" aria-busy="true" aria-label={t("Loading")}>
            <div className="fs-job fs-wait" />
            <div className="fs-job fs-wait" />
          </div>
        ) : jobs.length === 0 ? (
          <EmptyState
            icon="calendar"
            title={mine ? t("No jobs for you today") : t("No jobs today")}
            hint={t("Jobs scheduled for today show up here in the order of their start.")}
            {...(allJobs
              ? { action: { label: t("See all jobs"), onClick: () => (location.hash = allJobs) } }
              : {})}
          />
        ) : (
          <ol className="fs-day-list">
            {jobs.map((job) => {
              const step = nextStep(job);
              const blocked = needsSignature(job);
              const address = siteText(job);
              const map = address ? linkOf("address", address) : null;
              const phone = str(job.site_contact_phone);
              const tel = phone ? linkOf("tel", phone) : null;
              const start = clockText(job.scheduled_start_minute);
              const length = shownNumber("duration_minutes", job.planned_minutes);
              const state = str(job.state);
              const primary = job.id === nextId;
              return (
                <li key={job.id} className={`fs-job${state === "done" ? " is-done" : ""}`}>
                  <div className="fs-job-time">
                    <strong>{start || "—"}</strong>
                    {length && <span className="hint">{length}</span>}
                  </div>
                  <div className="fs-job-body">
                    <div className="fs-job-top">
                      <button type="button" className="fs-job-title" onClick={() => open(job)}>
                        {str(job.name) || t("Job")}
                      </button>
                      <Badge tone={stateTone(state)}>{optionLabel(state, stateField)}</Badge>
                      {(job.priority === "high" || job.priority === "urgent") && (
                        <Badge tone={job.priority === "urgent" ? "danger" : "warning"}>
                          {optionLabel(
                            str(job.priority),
                            model?.fields.find((f) => f.name === "priority"),
                          )}
                        </Badge>
                      )}
                    </div>
                    {!!job.customer_id && (
                      <p className="fs-job-customer">{d.refText("party", str(job.customer_id))}</p>
                    )}
                    {address && (
                      <p className="fs-job-line">
                        <Icon name="home" size={15} />
                        <span className="fs-job-text">
                          {map ? (
                            <a href={map} target="_blank" rel="noreferrer noopener">
                              {address}
                            </a>
                          ) : (
                            address
                          )}
                        </span>
                      </p>
                    )}
                    {(phone || str(job.site_contact_name)) && (
                      <p className="fs-job-line">
                        <Icon name="user" size={15} />
                        <span className="fs-job-text">
                          {str(job.site_contact_name)}
                          {tel && (
                            <a href={tel} className="fs-job-tel">
                              {phone}
                            </a>
                          )}
                        </span>
                      </p>
                    )}
                    {!mine && !!job.technician_id && (
                      <p className="fs-job-line hint">
                        <Icon name="users" size={15} />
                        {d.refText("hr_employee", str(job.technician_id))}
                      </p>
                    )}
                    <div className="fs-job-actions">
                      {step && !blocked && (
                        <Button
                          variant={primary ? "primary" : "secondary"}
                          loading={working === job.id}
                          disabled={!!working}
                          onClick={() => void run(job, step.action, step.done)}
                        >
                          {command(step.action) ? actionTitle(command(step.action)!) : step.label}
                        </Button>
                      )}
                      {blocked && (
                        <Button
                          variant={primary ? "primary" : "secondary"}
                          icon="edit"
                          onClick={() => open(job)}
                        >
                          {t("Get signature")}
                        </Button>
                      )}
                      <Button variant="ghost" onClick={() => open(job)}>
                        {t("Open job")}
                      </Button>
                    </div>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
        {app.id && allJobs && jobs !== null && jobs.length > 0 && (
          <p className="fs-day-foot">
            <a href={allJobs}>{t("All jobs")}</a>
          </p>
        )}
      </div>
    </section>
  );
}

function stateTone(state: string): "info" | "warning" | "success" | "neutral" {
  if (state === "done") return "success";
  if (state === "en_route" || state === "in_progress") return "warning";
  if (state === "scheduled") return "info";
  return "neutral";
}
