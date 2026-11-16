// What the Field Service screens know about a job: its model, the one next step of its state and
// whether the customer still has to sign. The server holds the rules (the actions' `from` states
// and the `complete` assertion); this only picks the button to draw.
import { t } from "@obop/ui";
import type { Row } from "../../api.ts";

export const JOB = "field_service_job";
export const PART = "field_service_part";
/** The route name of the technician's day inside the Field Service app. */
export const MY_DAY = "my-day";

const str = (v: unknown) => (typeof v === "string" ? v.trim() : "");

export type Step = { action: string; label: string; done: string };

/** The next action of a job in its state, or null when it has none (done, cancelled). */
export function nextStep(job: Row): Step | null {
  switch (job.state) {
    case "scheduled":
      return { action: "depart", label: t("On my way"), done: t("On the way") };
    case "en_route":
      return { action: "start", label: t("Start work"), done: t("Work started") };
    case "in_progress":
      return { action: "complete", label: t("Complete job"), done: t("Job done") };
    default:
      return null;
  }
}

/** The job is in progress and needs the customer's sign-off before it can be completed. */
export const needsSignature = (job: Row): boolean =>
  job.state === "in_progress" &&
  job.signature_required === true &&
  (!str(job.signed_by) || !str(job.signed_on));

/** The site on one line: name, street, postcode and city, country. */
export function siteText(job: Row): string {
  const street = str(job.site_address).replace(/\s*\n\s*/g, ", ");
  const town = [str(job.site_postcode), str(job.site_city)].filter(Boolean).join(" ");
  return [street, town, str(job.site_country)].filter(Boolean).join(", ");
}
