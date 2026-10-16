import { useMemo, useState } from "react";
import { call, type Kind, type ModuleMeta } from "./api.ts";
import { label as labelOf, modelLabel } from "./format.ts";
import { href } from "./route.ts";

type Props = {
  token: string;
  modules: ModuleMeta[];
  onModulesChanged: () => void;
  notify: (t: string, tone?: "ok" | "error" | "info") => void;
};

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
const CLASSES = [
  "none",
  "internal",
  "public_directory",
  "confidential_hr",
  "special_category",
  "financial",
];
const KINDS: [Kind, string][] = [
  ["text", "Text"],
  ["longtext", "Long text"],
  ["number", "Number"],
  ["money", "Money"],
  ["date", "Date"],
  ["bool", "Yes / No"],
  ["select", "Choice"],
  ["ref", "Link to a record"],
];

export type DField = {
  name: string;
  description: string;
  class: string;
  personal: boolean;
  kind: Kind;
  /** Comma separated, for a choice. */
  options: string;
  /** Target model, for a link. */
  to: string;
  required: boolean;
};

const blank = (): DField => ({
  name: "",
  description: "",
  class: "none",
  personal: false,
  kind: "text",
  options: "",
  to: "",
  required: false,
});

const listOf = (s: string) =>
  s
    .split(",")
    .map((x) => slug(x))
    .filter(Boolean);

/** The stored name: money is kept in `_minor` units, a link ends in `_id`. */
export function fieldName(f: Pick<DField, "name" | "kind">): string {
  const n = slug(f.name);
  if (!n) return n;
  if (f.kind === "money" && !n.endsWith("_minor")) return `${n}_minor`;
  if (f.kind === "ref" && !n.endsWith("_id")) return `${n}_id`;
  return n;
}

const q = (s: string) => JSON.stringify(s);

export function manifest(
  name: string,
  modelName: string,
  fields: DField[],
  statuses: string[],
): string {
  const id = slug(name);
  const model = slug(modelName) === id ? id : `${id}_${slug(modelName)}`;
  const lines = [
    "[module]",
    `id = ${q(id)}`,
    `version = "0.1.0"`,
    `name = ${q(name.trim())}`,
    `description = ${q(`${name.trim()}: ${modelName.trim()} records`)}`,
    `author = "Built in obop"`,
    "",
    "[[model]]",
    `name = ${q(model)}`,
    `description = ${q(modelName.trim())}`,
    `read_group = ${q(`${id}.user`)}`,
    `write_group = ${q(`${id}.manager`)}`,
  ];
  const hasState = statuses.length > 1 && !fields.some((f) => fieldName(f) === "state");
  const all: DField[] = [...fields];
  if (hasState)
    all.push({
      ...blank(),
      name: "state",
      kind: "select",
      options: statuses.join(","),
      description: `Status: ${statuses.join(", ")}`,
    });
  for (const f of all) {
    const n = fieldName(f);
    lines.push(
      "",
      "  [[model.field]]",
      `  name = ${q(n)}`,
      `  description = ${q(f.description || labelOf(n))}`,
      `  class = ${q(f.class)}`,
    );
    if (f.personal) lines.push("  personal_data = true");
    if (f.required) lines.push("  required = true");
    if (f.kind !== "text") lines.push(`  kind = ${q(f.kind)}`);
    if (f.kind === "select") lines.push(`  options = [${listOf(f.options).map(q).join(", ")}]`);
    if (f.kind === "ref" && f.to) lines.push(`  to = ${q(f.to)}`);
  }
  if (hasState)
    for (let i = 1; i < statuses.length; i++) {
      lines.push(
        "",
        "  [[model.action]]",
        `  name = ${q(`to_${slug(statuses[i]!)}`)}`,
        `  description = ${q(`Move from ${statuses[i - 1]} to ${statuses[i]}`)}`,
        `  field = "state"`,
        `  from = ${q(statuses[i - 1]!)}`,
        `  to = ${q(statuses[i]!)}`,
      );
    }
  return lines.join("\n") + "\n";
}

export function BuildTab({ token, modules, onModulesChanged, notify }: Props) {
  const [name, setName] = useState("Fleet check");
  const [modelName, setModelName] = useState("Inspection");
  const [fields, setFields] = useState<DField[]>([
    { ...blank(), name: "title", description: "What was inspected", required: true },
    { ...blank(), name: "inspector", description: "Who did it", class: "internal", personal: true },
    { ...blank(), name: "due", description: "When it is due", kind: "date" },
  ]);
  const [statuses, setStatuses] = useState("open, done");
  const [error, setError] = useState<string | null>(null);
  const st = useMemo(() => listOf(statuses), [statuses]);
  const named = fields.filter((f) => f.name.trim());
  const targets = useMemo(
    () =>
      modules
        .filter((m) => m.id !== "platform")
        .flatMap((m) =>
          m.models.map((x) => ({ value: x.name, text: `${m.name}: ${modelLabel(x.name, m.id)}` })),
        ),
    [modules],
  );
  const text = useMemo(
    () => manifest(name || "My module", modelName || "Record", named, st),
    [name, modelName, named, st],
  );
  const id = slug(name);
  const upd = (i: number, patch: Partial<DField>) =>
    setFields(fields.map((f, j) => (j === i ? { ...f, ...patch } : f)));

  function problem(): string | null {
    if (!id) return "Give the module a name";
    if (named.length === 0 && st.length < 2) return "Add at least one field";
    const names = named.map(fieldName);
    const dup = names.find((n, i) => names.indexOf(n) !== i);
    if (dup) return `Two fields are called ${dup}`;
    if (names.includes("id")) return "A field cannot be called id";
    for (const f of named) {
      if (f.kind === "select" && listOf(f.options).length === 0)
        return `Choices for ${fieldName(f)} need at least one option`;
      if (f.kind === "ref" && !f.to) return `Pick the record type that ${fieldName(f)} links to`;
    }
    return null;
  }

  return (
    <div className="two">
      <div className="stack">
        <p className="hint">
          Describe a model and install it as a module. It gets the API, agent tools, permissions and
          privacy rules like every other app.{" "}
          <a className="reflink" href={href({ app: "customize", model: "guide", tab: "guide" })}>
            How to write a custom module?
          </a>
        </p>
        <label className="field">
          <span className="flabel">Module name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="field">
          <span className="flabel">Record type</span>
          <input value={modelName} onChange={(e) => setModelName(e.target.value)} />
        </label>
        <h3>Fields</h3>
        <ul className="rows">
          {fields.map((f, i) => (
            <li key={i} className="row wrap fieldrow">
              <div className="frow">
                <input
                  value={f.name}
                  onChange={(e) => upd(i, { name: e.target.value })}
                  placeholder="name"
                  aria-label="Field name"
                />
                <select
                  value={f.kind}
                  onChange={(e) => upd(i, { kind: e.target.value as Kind })}
                  aria-label="Field kind"
                >
                  {KINDS.map(([k, l]) => (
                    <option key={k} value={k}>
                      {l}
                    </option>
                  ))}
                </select>
                <button
                  className="icon-btn"
                  onClick={() => setFields(fields.filter((_, j) => j !== i))}
                  aria-label="Remove field"
                >
                  ×
                </button>
              </div>
              {f.kind === "select" && (
                <input
                  value={f.options}
                  onChange={(e) => upd(i, { options: e.target.value })}
                  placeholder="options, comma separated"
                  aria-label="Options"
                />
              )}
              {f.kind === "ref" && (
                <select
                  value={f.to}
                  onChange={(e) => upd(i, { to: e.target.value })}
                  aria-label="Target model"
                >
                  <option value="">Links to…</option>
                  {targets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.text}
                    </option>
                  ))}
                </select>
              )}
              <div className="frow">
                <input
                  value={f.description}
                  onChange={(e) => upd(i, { description: e.target.value })}
                  placeholder="description"
                  aria-label="Field description"
                />
                <select
                  value={f.class}
                  onChange={(e) => upd(i, { class: e.target.value })}
                  aria-label="Data class"
                >
                  {CLASSES.map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </div>
              <div className="frow">
                <label className="check">
                  <input
                    type="checkbox"
                    checked={f.required}
                    onChange={(e) => upd(i, { required: e.target.checked })}
                  />
                  Required
                </label>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={f.personal}
                    onChange={(e) => upd(i, { personal: e.target.checked })}
                  />
                  Personal data
                </label>
                {fieldName(f) !== slug(f.name) && (
                  <span className="hint">stored as {fieldName(f)}</span>
                )}
              </div>
            </li>
          ))}
        </ul>
        <button className="btn" onClick={() => setFields([...fields, blank()])}>
          Add field
        </button>
        <label className="field">
          <span className="flabel">Status field (optional, statuses comma separated)</span>
          <input
            value={statuses}
            onChange={(e) => setStatuses(e.target.value)}
            placeholder="open, in progress, done"
          />
          <span className="hint">
            Adds a choice field called state, one action per step and a status bar on the form.
          </span>
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="actionrow">
          <button
            className="btn primary"
            onClick={async () => {
              const p = problem();
              if (p) return setError(p);
              const r = await call(token, "module.install", { manifest: text });
              if (r.kind === "ok") {
                setError(null);
                notify(`Installed ${name}`);
                onModulesChanged();
                location.hash = href({ app: id });
              } else setError(r.kind === "error" ? r.message : "Waiting for a human to approve");
            }}
          >
            Create and install
          </button>
        </div>
      </div>
      <div className="stack">
        <h3>obop.toml</h3>
        <pre className="code">{text}</pre>
      </div>
    </div>
  );
}
