// Every sentence of the Approvals page, built from the plan's data, the page's own keys and the
// `/api/meta` labels. The server sends ids and data only: no sentence of the page comes from it
// except the text of a refusal, which goes through serverMessage.ts as on every other screen.
import { fmt, inSentence, t } from "@obop/ui";
import type { ModelInfo } from "../data.tsx";
import { dayInZone } from "../instants.ts";
import { getTime, type FieldMeta } from "../api.ts";
import { formatDate, instantText, isEmpty, modelLabel, plainText } from "../format.ts";
import { actionTitle, fieldTitle, fieldTitleByName, modelTitle } from "../labels.ts";
import type { PlanFail, PlanRow } from "./planApi.ts";

/** The labels the page reads from `/api/meta`; a model the person cannot open is named from its id. */
export type Names = {
  model: (model: string) => string;
  action: (command: string) => string;
  field: (model: string, field: string) => { title: string; meta?: FieldMeta };
};

export function namesFrom(models: Map<string, ModelInfo>): Names {
  return {
    model: (name) => {
      const info = models.get(name);
      return info ? modelTitle(info.model, info.module.id) : modelLabel(name, "");
    },
    action: (command) => {
      const [model = "", act = ""] = command.split(".");
      const c = models.get(model)?.model.actions.find((a) => a.id === command);
      return c ? actionTitle(c) : actionTitle({ id: `${model}.${act}` });
    },
    field: (model, name) => {
      const meta = models.get(model)?.model.fields.find((f) => f.name === name);
      return meta ? { title: fieldTitle(meta), meta } : { title: fieldTitleByName(name) };
    },
  };
}

/** The record's own title, only when the server gave one and the record line is not null. */
const recordOf = (row: Pick<PlanRow, "record_id" | "record_title">): string =>
  row.record_id && row.record_title ? row.record_title : "";

/**
 * What the plan does, as the first line of a row: "Post invoice “INV-0042”", "Create contact",
 * "Delete lead “Acme”". A capital first letter; `inSentence` makes the nouns read inside a sentence.
 */
export function thingOf(row: PlanRow, n: Names): string {
  const model = inSentence(n.model(row.model));
  const record = recordOf(row);
  switch (row.kind) {
    case "create":
      return t("Create {model}", { model });
    case "bulk_create":
      return t("Create {model} records in bulk", { model });
    case "delete":
      return record
        ? t("Delete {model} “{record}”", { model, record })
        : t("Delete {model}", { model });
    case "update":
      return record
        ? t("Change {model} “{record}”", { model, record })
        : t("Change {model}", { model });
    default: {
      const action = n.action(row.command);
      return record
        ? t("{action} {model} “{record}”", { action, model, record })
        : t("{action} {model}", { action, model });
    }
  }
}

/** The detail's heading: "An agent asks to post invoice “INV-0042”." */
export function askOf(row: PlanRow, n: Names): string {
  return t("An agent asks to {do}.", { do: inSentence(thingOf(row, n)) });
}

/** The filled button names the act: "Approve and delete", "Approve and post", plain "Approve". */
export function approveLabel(row: PlanRow, n: Names): string {
  switch (row.kind) {
    case "create":
    case "bulk_create":
      return t("Approve and create");
    case "delete":
      return t("Approve and delete");
    case "update":
      return t("Approve and save");
    case "action":
      return t("Approve and {action}", { action: inSentence(n.action(row.command)) });
    default:
      return t("Approve");
  }
}

/** Who asked: "Ada's agent", or "Your own agent" when the proposer is the person's own agent. */
export function whoOf(row: Pick<PlanRow, "owner" | "own_agent">): string {
  if (row.own_agent) return t("Your own agent");
  return row.owner ? t("{name}'s agent", { name: row.owner }) : t("An agent");
}

/** "2 hours ago"; "time unknown" for a plan made before receipts existed. */
export function whenOf(iso: string | null | undefined): string {
  return iso ? fmt.relative(iso) : t("time unknown");
}

/** "21 Nov 2026" in the instance's time zone. */
export function dateOf(iso: string | null | undefined): string {
  return iso ? formatDate(dayInZone(iso, getTime().zone)) : "";
}

/** The second line of a waiting row: "Ada's agent · 2 hours ago · expires in 5 days". */
export function waitingLine(row: PlanRow): string {
  const parts = [whoOf(row), whenOf(row.created_at)];
  if (row.expires_at) parts.push(t("expires {when}", { when: fmt.relative(row.expires_at) }));
  return parts.join(" · ");
}

/** The words of a decided plan's status, short, for a chip. */
export function statusWord(status: PlanRow["status"]): string {
  switch (status) {
    case "approved":
      return t("Approved");
    case "rejected":
      return t("Rejected");
    case "failed":
      return t("Refused by the system");
    case "expired":
      return t("Expired");
    default:
      return t("Waiting");
  }
}

/**
 * What became of a decided plan, as a sentence: "Ada approved this 2 minutes ago." From the
 * plan's own row, or from a 409 `already_decided` answer.
 */
export function decidedSentence(d: {
  status?: string;
  decided_by?: string | null;
  decided_at?: string | null;
  expires_at?: string | null;
}): string {
  const name = d.decided_by || t("Somebody");
  // "2 minutes ago"; a decision without a time (none is written without one today) says no time.
  const when = d.decided_at ? fmt.relative(d.decided_at) : "";
  switch (d.status) {
    case "approved":
      return when
        ? t("{name} approved this {when}.", { name, when })
        : t("{name} approved this.", { name });
    case "rejected":
      return when
        ? t("{name} rejected this {when}.", { name, when })
        : t("{name} rejected this.", { name });
    case "failed":
      return t("{name} approved this, and the system refused it. Nothing was changed.", { name });
    case "expired": {
      const date = dateOf(d.decided_at ?? d.expires_at);
      return date
        ? t(
            "This request expired on {date}. Nothing was changed. Ask the agent to propose it again.",
            {
              date,
            },
          )
        : t("This request expired. Nothing was changed. Ask the agent to propose it again.");
    }
    default:
      return t("This request is not waiting any more. It may have been decided or it expired.");
  }
}

/** The same sentence for hidden, unknown, decided long ago and not yours (no leak). */
export const goneSentence = (): string =>
  t("This request is not waiting any more. It may have been decided or it expired.");

/** The sentence of a 409 `already_decided`. */
export const alreadyDecidedSentence = (f: PlanFail, expiresAt?: string | null): string =>
  decidedSentence({
    status: f.state,
    decided_by: f.decided_by,
    decided_at: f.decided_at,
    expires_at: expiresAt,
  });

/** A value of the change view as the person reads it; "Not set" for an empty one. */
export function valueText(
  v: unknown,
  meta: FieldMeta | undefined,
  refText: (model: string, id: string) => string,
): string {
  if (isEmpty(v)) return t("Not set");
  if (meta?.kind === "datetime") return instantText(v);
  if (meta?.kind === "date") return formatDate(v);
  if (meta) return plainText(meta, v, refText);
  if (typeof v === "boolean") return v ? t("Yes") : t("No");
  return typeof v === "object" ? JSON.stringify(v) : String(v);
}
