// Server messages (validation errors, guard refusals, sign-in failures) arrive in English. The known,
// stable patterns (docs/MODULES.md section 3.5 and the D47/D48/D49 error shapes) are mapped to
// translatable messages here; anything else is shown as received. When the server learns to send a
// code with its messages, replace the patterns by that code.
import { getLang, t, tx } from "@obop/ui";
import type { ModuleMeta } from "./api.ts";
import { label, optionLabel } from "./format.ts";
import { fieldTitle, modelTitle } from "./labels.ts";

let modules: ModuleMeta[] = [];
/** Lets messages name models and fields the way the interface does. */
export function registerMeta(m: ModuleMeta[]) {
  modules = m;
}

function modelName(name: string): string {
  for (const m of modules) {
    const hit = m.models.find((x) => x.name === name);
    if (hit) return modelTitle(hit, m.id);
  }
  return tx("model", label(name));
}

function fieldName(model: string, field: string): string {
  for (const m of modules) {
    const f = m.models.find((x) => x.name === model)?.fields.find((x) => x.name === field);
    if (f) return fieldTitle(f);
  }
  return tx("field", label(field));
}

function values(list: string): string {
  return list
    .split(/,\s*/)
    .map((v) => optionLabel(v.replace(/^"|"$/g, "")))
    .join(", ");
}

type Rule = [RegExp, (m: RegExpExecArray) => string];

const RULES: Rule[] = [
  [
    /^(\w+)\.(\w+) is required and must be set before (\w+) can change to "([^"]*)"$/,
    (m) =>
      t("{field} is required before {state} can change to {value}", {
        field: fieldName(m[1]!, m[2]!),
        state: fieldName(m[1]!, m[3]!),
        value: optionLabel(m[4]!),
      }),
  ],
  [
    /^(\w+)\.(\w+) is required$/,
    (m) => t("{field} is required", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be a string$/,
    (m) => t("{field} must be text", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be an integer$/,
    (m) => t("{field} must be a whole number", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be an integer in minor currency units$/,
    (m) => t("{field} must be an amount", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be a real calendar date as YYYY-MM-DD$/,
    (m) => t("{field} must be a real date (YYYY-MM-DD)", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be true or false$/,
    (m) => t("{field} must be yes or no", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+) must be one of: (.*)$/,
    (m) =>
      t("{field} must be one of: {options}", {
        field: fieldName(m[1]!, m[2]!),
        options: values(m[3]!),
      }),
  ],
  [
    /^(\w+)\.(\w+) must be the id of an existing (\w+) record$/,
    (m) =>
      t("{field} must point to an existing {model} record", {
        field: fieldName(m[1]!, m[2]!),
        model: modelName(m[3]!),
      }),
  ],
  [
    /^(\w+)\.(\w+) cannot change from (\S+) to "([^"]*)"; from \S+ it can only go to: (.*)$/,
    (m) =>
      t("{field} cannot change from {from} to {to}; from {from} it can only go to: {allowed}", {
        field: fieldName(m[1]!, m[2]!),
        from: optionLabel(m[3]!),
        to: optionLabel(m[4]!),
        allowed: values(m[5]!),
      }),
  ],
  [
    /^(\w+)\.(\w+) cannot change from (\S+) to "([^"]*)": no transition leaves \S+$/,
    (m) =>
      t("{field} cannot change from {from} to {to}: no action leads away from {from}", {
        field: fieldName(m[1]!, m[2]!),
        from: optionLabel(m[3]!),
        to: optionLabel(m[4]!),
      }),
  ],
  [
    /^unknown field (\w+)\.(\w+)$/,
    (m) => t("Unknown field: {field}", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^cannot delete (\w+) (\S+): still referenced by (.*)$/,
    (m) =>
      t("This {model} cannot be deleted: other records still refer to it ({details})", {
        model: modelName(m[1]!),
        details: m[3]!
          .split("; ")
          .map((part) => part.replace(/^(\w+) \(/, (_, n: string) => `${modelName(n)} (`))
          .join("; "),
      }),
  ],
  [/^no (\w+) \S+$/, (m) => t("No such {model} record", { model: modelName(m[1]!) })],
  [/^invalid username or password$/, () => t("Invalid username or password")],
  [
    /^too many attempts, try again in a minute$/,
    () => t("Too many attempts. Try again in a minute."),
  ],
  [/^invalid token$/, () => t("The value is not valid for this field")],
  [/^not found$/, () => t("Not found")],
];

/** Translate a server message when its pattern is known, else return it unchanged. */
export function serverMessage(raw: string | undefined | null): string {
  if (!raw) return "";
  // The server's own words are English: in English they are shown exactly as received.
  if (getLang() === "en") return raw;
  for (const [re, fn] of RULES) {
    const m = re.exec(raw);
    if (m) return fn(m);
  }
  return raw;
}
