// Server messages (validation errors, guard refusals, sign-in failures) arrive in English. The known,
// stable patterns (docs/MODULES.md section 3.5 and the D47/D48/D49 error shapes) are mapped to
// translatable messages here; anything else is shown as received. When the server learns to send a
// code with its messages, replace the patterns by that code.
import { getLang, t, tx } from "@obop/ui";
import type { FieldMeta, ModuleMeta } from "./api.ts";
import { label, optionLabel } from "./format.ts";
import { appTitle, fieldTitle, modelTitle } from "./labels.ts";

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

/** The field of a model as `/api/meta` described it, or `undefined` when either is unknown. */
function findField(model: string, field: string): FieldMeta | undefined {
  for (const m of modules) {
    const f = m.models.find((x) => x.name === model)?.fields.find((x) => x.name === field);
    if (f) return f;
  }
  return undefined;
}

/** The field a rule names as `model.field` in its first two groups. */
const named = (m: RegExpExecArray) => findField(m[1]!, m[2]!);

function fieldName(model: string, field: string): string {
  const f = findField(model, field);
  return f ? fieldTitle(f) : tx("field", label(field));
}

/** The option value without the quotes the server puts around it. */
function unquote(raw: string): string {
  return raw.replace(/^"|"$/g, "");
}

/**
 * An option value as the server quotes it ("draft"), or "(empty)", shown as the interface would:
 * with the package's own words when `field` is known (its `option_labels`), else the generic table.
 */
function opt(raw: string, field?: FieldMeta): string {
  if (raw === "(empty)" || raw === "empty") return t("empty");
  return optionLabel(unquote(raw), field);
}

/**
 * The select field a sentence means when it names the field but not the model ("stage must be new
 * to do this", from a state change): a select of that name in the registered modules that offers
 * every value the sentence quotes. It is used only when all such fields would show those values
 * the same way; when models disagree the field is unknown and the generic table applies.
 */
function selectNamed(name: string, ...raws: string[]): FieldMeta | undefined {
  const wanted = raws.filter((r) => r !== "(empty)" && r !== "empty").map(unquote);
  const hits: FieldMeta[] = [];
  for (const m of modules)
    for (const model of m.models) {
      const f = model.fields.find((x) => x.name === name && x.kind === "select");
      if (f && wanted.every((v) => f.options?.includes(v))) hits.push(f);
    }
  const first = hits[0];
  if (!first) return undefined;
  const same = hits.every((f) => wanted.every((v) => optionLabel(v, f) === optionLabel(v, first)));
  return same ? first : undefined;
}

/** A module by the name the interface gives it (its id when it is not installed). */
function moduleName(id: string): string {
  const m = modules.find((x) => x.id === id);
  return m ? appTitle(m) : tx("app", label(id));
}

function values(list: string, field?: FieldMeta): string {
  return list
    .split(/,\s*/)
    .map((v) => optionLabel(unquote(v), field))
    .join(", ");
}

/**
 * The translated message for a 503 that says the database was busy, unreachable or too slow
 * (D74), or `null` for any other answer. Screens that decide by status (sign-in) use it so that
 * a busy server is not shown as a wrong password.
 */
export function storageFaultMessage(status: number, code: string | undefined): string | null {
  if (status !== 503) return null;
  return STORAGE_FAULTS[code ?? ""]?.() ?? null;
}

/**
 * The code words of a refused database call (D74), in the person's language. They are also shown
 * in English: the server's `storage_busy` is a code, not a sentence.
 */
const STORAGE_FAULTS: Record<string, () => string> = {
  storage_busy: () => t("The server is busy. Try again in a moment."),
  storage_unavailable: () => t("The database cannot be reached right now. Try again in a moment."),
  storage_timeout: () =>
    t("The database took too long to answer. Try again, or ask for less at once."),
};

type Rule = [RegExp, (m: RegExpExecArray) => string];

const RULES: Rule[] = [
  [
    /^(\w+)\.(\w+) is required and must be set before (\w+) can change to "([^"]*)"$/,
    (m) =>
      t("{field} is required before {state} can change to {value}", {
        field: fieldName(m[1]!, m[2]!),
        state: fieldName(m[1]!, m[3]!),
        value: optionLabel(m[4]!, findField(m[1]!, m[3]!)),
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
        options: values(m[3]!, named(m)),
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
        from: opt(m[3]!, named(m)),
        to: optionLabel(m[4]!, named(m)),
        allowed: values(m[5]!, named(m)),
      }),
  ],
  [
    /^(\w+)\.(\w+) cannot change from (\S+) to "([^"]*)": no transition leaves \S+$/,
    (m) =>
      t("{field} cannot change from {from} to {to}: no action leads away from {from}", {
        field: fieldName(m[1]!, m[2]!),
        from: opt(m[3]!, named(m)),
        to: optionLabel(m[4]!, named(m)),
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
  // Transport failures the app itself reports (net.ts).
  [
    /^Could not reach the server$/,
    () => t("Could not reach the server. Check your connection and try again."),
  ],
  [
    /^The server had a problem \(status (\d+)\)$/,
    (m) =>
      t("The server had a problem (status {status}). Try again in a moment.", { status: m[1]! }),
  ],
  // Governed fields (D48).
  [
    /^(\w+)\.(\w+) starts as "([^"]*)" when a record is created; it moves on through the model's actions$/,
    (m) =>
      t("{field} starts as {value} when a record is created; it moves on through the actions", {
        field: fieldName(m[1]!, m[2]!),
        value: opt(m[3]!, named(m)),
      }),
  ],
  [
    /^(\w+)\.(\w+): moving from (.+) to "([^"]*)" is the (\w+) action, which needs the ([\w.]+) group$/,
    (m) =>
      t(
        "{field}: moving from {from} to {to} is the action {action}, which needs the group {group}",
        {
          field: fieldName(m[1]!, m[2]!),
          from: opt(m[3]!, named(m)),
          to: opt(m[4]!, named(m)),
          action: tx("action", label(m[5]!)),
          group: m[6]!,
        },
      ),
  ],
  [
    /^(\w+) must be (\S+) to do this, it is (.+)$/,
    (m) => {
      // The sentence names the field but not its model: use the field of the metadata when it is the
      // same one wherever it occurs, else the generic option table.
      const f = selectNamed(m[1]!, m[2]!, m[3]!);
      return t("{field} must be {from} to do this, it is {current}", {
        field: f ? fieldTitle(f) : tx("field", label(m[1]!)),
        from: opt(m[2]!, f),
        current: opt(m[3]!, f),
      });
    },
  ],
  // Server-side queries (D49) and summaries (D4).
  [/^unknown op "([^"]*)" \(use .*\)$/, (m) => t("Unknown filter operator {op}", { op: m[1]! })],
  [
    /^unknown option "([^"]*)" \(use .*\)$/,
    (m) => t("Unknown search option {option}", { option: m[1]! }),
  ],
  [
    /^at most (\d+) filter conditions$/,
    (m) => t("At most {max} filter conditions", { max: m[1]! }),
  ],
  [/^at most (\d+) order keys$/, (m) => t("At most {max} sort keys", { max: m[1]! })],
  [
    /^limit must be an integer from 1 to (\d+)$/,
    (m) => t("The page size must be a whole number from 1 to {max}", { max: m[1]! }),
  ],
  [/^offset must be zero or more$/, () => t("The offset must be zero or more")],
  [
    /^filter on (\w+)\.(\w+): contains only works on text fields$/,
    (m) => t("“Contains” only works on text fields ({field})", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^filter on (\w+)\.(\w+): value must be (.*)$/,
    (m) => t("The filter value for {field} is not valid", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^aggregate would produce more than (\d+) groups; narrow the filter or group by a coarser field$/,
    (m) =>
      t(
        "This summary would have more than {max} groups. Narrow the search or group by a coarser field.",
        {
          max: m[1]!,
        },
      ),
  ],
  [
    /^aggregate is only available on search$/,
    () => t("Summaries are only available through search"),
  ],
  // Installing and removing modules (D55).
  [
    /^(\w+) needs (\w+) (.+), which is not installed and not offered by the marketplace$/,
    (m) =>
      t(
        "{module} needs {dependency} {requirement}, which is not installed and not offered by the marketplace",
        {
          module: moduleName(m[1]!),
          dependency: moduleName(m[2]!),
          requirement: m[3]!,
        },
      ),
  ],
  [
    /^(\w+) needs (\w+) (.+), but the marketplace offers \2 (\S+)$/,
    (m) =>
      t("{module} needs {dependency} {requirement}, but the marketplace offers version {version}", {
        module: moduleName(m[1]!),
        dependency: moduleName(m[2]!),
        requirement: m[3]!,
        version: m[4]!,
      }),
  ],
  [
    /^(\w+) (\S+) needs core (.+), you run (\S+)$/,
    (m) =>
      t("{module} {version} needs core {requirement}, you run {core}", {
        module: moduleName(m[1]!),
        version: m[2]!,
        requirement: m[3]!,
        core: m[4]!,
      }),
  ],
  [/^dependency cycle: (.+)$/, (m) => t("Dependency cycle: {path}", { path: m[1]! })],
  [
    /^(\w+) (\S+) is already installed$/,
    (m) =>
      t("{module} {version} is already installed", { module: moduleName(m[1]!), version: m[2]! }),
  ],
  [
    /^(\w+) (\S+) is already installed; (\S+) would be a downgrade, which is refused$/,
    (m) =>
      t(
        "{module} {version} is already installed; version {other} would be a downgrade, which is refused",
        {
          module: moduleName(m[1]!),
          version: m[2]!,
          other: m[3]!,
        },
      ),
  ],
  [
    /^(\w+) is a built-in module( and is part of the core)?$/,
    (m) =>
      t("{module} is a built-in module and is part of the core", { module: moduleName(m[1]!) }),
  ],
  [
    /^model (\w+) already exists$/,
    (m) => t("Another module already has a model named {model}", { model: modelName(m[1]!) }),
  ],
  [
    /^(\w+) is required by (.+); uninstall those first$/,
    (m) =>
      t("{module} is required by {others}; uninstall those first", {
        module: moduleName(m[1]!),
        others: m[2]!,
      }),
  ],
  [
    /^upgrading (\w+) to (\S+) would break (\w+), which needs (\w+) (.+)$/,
    (m) =>
      t("Upgrading {module} to {version} would break {other}, which needs {need}", {
        module: moduleName(m[1]!),
        version: m[2]!,
        other: moduleName(m[3]!),
        need: `${moduleName(m[4]!)} ${m[5]!}`,
      }),
  ],
  // Module manifests (docs/MODULES.md section 3.5).
  [
    /^invalid obop\.toml: (.*)$/,
    (m) => t("This is not a valid module file: {detail}", { detail: m[1]! }),
  ],
  [/^manifest is larger than 256 KiB$/, () => t("The module file is larger than 256 KiB")],
  [
    /^module id "([^"]*)" must be lowercase letters, digits and _, starting with a letter$/,
    (m) =>
      t("The module id {id} must be lowercase letters, digits and _, starting with a letter", {
        id: m[1]!,
      }),
  ],
  [/^version must look like 1\.2\.3.*$/, () => t("The version must look like 1.2.3")],
  [
    /^summary must be one line of at most 140 characters$/,
    () => t("The summary must be one line of at most 140 characters"),
  ],
  [
    /^(homepage|repository|documentation|support) must be an https:\/\/ URL \(got "(.*)"\)$/,
    (m) => t("{key} must be an https:// address (got {value})", { key: m[1]!, value: m[2]! }),
  ],
  [/^module needs a name and a description$/, () => t("The module needs a name and a description")],
  [
    /^declarative packages run no code and may not declare network destinations$/,
    () => t("A module made of data runs no code and may not declare network destinations"),
  ],
  [/^package is too large/, () => t("The module is too large: at most 50 models or 200 fields")],
  [
    /^model "([^"]*)" must be "([^"]*)" or start with "([^"]*)_"$/,
    (m) =>
      t("The model {model} must be named {id} or start with {id}_", { model: m[1]!, id: m[2]! }),
  ],
  [
    /^permission group "([^"]*)" must look like module\.role$/,
    (m) => t("The permission group {group} must look like module.role", { group: m[1]! }),
  ],
  [
    /^(\w+)\.(\w+): a select needs options$/,
    (m) => t("{field}: a select needs options", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [
    /^(\w+)\.(\w+): a ref needs `to`, the target model$/,
    (m) => t("{field}: a ref needs `to`, the target model", { field: fieldName(m[1]!, m[2]!) }),
  ],
  [/^.*: field has no description$/, () => t("Every field needs a description")],
  [/^.*: command has no description$/, () => t("Every action needs a description")],
  [/^.*: model has no description$/, () => t("Every model needs a description")],
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
  // A refused database call is a code word (D74), a sentence in every language, English too.
  const fault = STORAGE_FAULTS[raw];
  if (fault) return fault();
  // The server's own words are English: in English they are shown exactly as received.
  if (getLang() === "en") return raw;
  for (const [re, fn] of RULES) {
    const m = re.exec(raw);
    if (m) return fn(m);
  }
  return raw;
}
