// Field Service: "My day" in the sidebar (today's jobs of the signed-in technician, in order of
// start; where the app opens on a phone), and on a job the customer's sign-off and the printed job
// report (apps/field_service, UI plan 3.5).
import { lazy } from "react";
import { t } from "@obop/ui";
import { registerAppPages, registerRecordExtension } from "../extensions.ts";
import { href } from "../route.ts";
import { setSignedInUser } from "../apps/hr/LeavePage.tsx";
import { CompleteJob, JobReport, PrintJob, SignOff } from "../apps/field_service/JobExtras.tsx";
import { JOB, MY_DAY } from "../apps/field_service/job.ts";

const MyDay = lazy(() =>
  import("../apps/field_service/MyDay.tsx").then((m) => ({ default: m.MyDay })),
);

const hasJobs = (models: readonly { name: string }[]) => models.some((m) => m.name === JOB);
/** A phone-sized screen: the technician's day is where the app opens. */
const narrow = () =>
  typeof matchMedia === "function" && matchMedia("(max-width: 47.99rem)").matches;

registerAppPages("field_service", {
  use: (ctx) => {
    if (!ctx || !hasJobs(ctx.app.module.models)) return {};
    setSignedInUser(ctx.email);
    // On a phone the app itself opens on the day (the launcher and `#/field_service` alike).
    const here = ctx.route.model === MY_DAY || (!ctx.route.model && narrow());
    return {
      navBefore: [
        {
          key: MY_DAY,
          label: t("My day"),
          href: href({ app: ctx.app.id, model: MY_DAY }),
          title: t("Today's jobs, in order"),
          current: here,
        },
      ],
      page: here ? <MyDay app={ctx.app} /> : null,
    };
  },
  home: (app) =>
    hasJobs(app.module.models) && narrow() ? href({ app: app.id, model: MY_DAY }) : undefined,
});

// Complete is drawn by CompleteJob, so that it is not offered next to "Customer signs".
registerRecordExtension(JOB, {
  actions: [SignOff, CompleteJob, PrintJob],
  panels: [JobReport],
  overrides: [`${JOB}.complete`],
});
