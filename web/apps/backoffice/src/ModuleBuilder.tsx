import { useEffect, useMemo, useState } from "react";
import { msg, t } from "@obop/ui";
import { call, getCoreVersion, type Kind, type ModuleMeta } from "./api.ts";
import { label as labelOf } from "./format.ts";
import { appTitle, modelTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
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
  ["text", msg("Text")],
  ["longtext", msg("Long text")],
  ["number", msg("Number")],
  ["money", msg("Money")],
  ["date", msg("Date")],
  ["datetime", msg("Date and time")],
  ["decimal", msg("Exact decimal number")],
  ["bool", msg("Yes / No")],
  ["select", msg("Choice")],
  ["ref", msg("Link to a record")],
  ["image", msg("Image")],
  ["file", msg("File")],
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
  /** For an exact decimal: digits in all, digits after the point, and the import rounding. */
  precision: string;
  scale: string;
  round: string;
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
  precision: "12",
  scale: "2",
  round: "",
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

/** The core range a new module declares by default: the series the instance runs, `^0.2`. */
export function defaultCoreRange(core: string): string {
  const [major, minor] = core.split(".");
  return major && minor ? `^${major}.${minor}` : "^0.2";
}

/** What a module needs of the others (D63): module id to the versions of it that it supports. */
export type Needs = Record<string, string>;

export function manifest(
  name: string,
  modelName: string,
  fields: DField[],
  statuses: string[],
  coreRange = defaultCoreRange(getCoreVersion()),
  needs: Needs = {},
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
    // Every module says which core versions it runs on and which versions of the modules it
    // needs it supports; an update is checked against these ranges (D63).
    `requires_core = ${q(coreRange)}`,
    ...(Object.keys(needs).length > 0
      ? ["", "[module.dependencies]", ...Object.entries(needs).map(([m, r]) => `${m} = ${q(r)}`)]
      : []),
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
    if (f.kind === "decimal") {
      lines.push(`  precision = ${Number.parseInt(f.precision, 10) || 12}`);
      lines.push(`  scale = ${Number.parseInt(f.scale, 10) || 0}`);
      if (f.round) lines.push(`  round = ${q(f.round)}`);
    }
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
  const [name, setName] = useState(() => t("Fleet check"));
  const [modelName, setModelName] = useState(() => t("Inspection"));
  const [fields, setFields] = useState<DField[]>(() => [
    { ...blank(), name: "title", description: t("What was inspected"), required: true },
    {
      ...blank(),
      name: "inspector",
      description: t("Who did it"),
      class: "internal",
      personal: true,
    },
    { ...blank(), name: "due", description: t("When it is due"), kind: "date" },
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
          m.models.map((x) => ({
            value: x.name,
            text: `${appTitle(m)}: ${modelTitle(x, m.id)}`,
          })),
        ),
    [modules],
  );
  // The versions of what is installed, so the ranges the module declares start from what it was
  // built against: the core's series, and for each module it links to, `^<installed version>`.
  const [core, setCore] = useState(() => defaultCoreRange(getCoreVersion()));
  const [versions, setVersions] = useState<Record<string, string>>({});
  useEffect(() => {
    let live = true;
    void call(token, "module.list").then((r) => {
      if (live && r.kind === "ok")
        setVersions(
          Object.fromEntries(
            (r.value as { id: string; version: string; source: string }[]).map((m) => [
              m.id,
              m.source === "builtin" ? getCoreVersion() || m.version : m.version,
            ]),
          ),
        );
    });
    return () => {
      live = false;
    };
  }, [token]);
  const owner = useMemo(() => {
    const m = new Map<string, string>();
    for (const mod of modules) for (const x of mod.models) m.set(x.name, mod.id);
    return m;
  }, [modules]);
  const needs: Needs = useMemo(() => {
    const out: Needs = {};
    const id0 = slug(name);
    for (const f of named) {
      const o = f.kind === "ref" && f.to ? owner.get(f.to) : undefined;
      if (!o || o === id0 || o === "platform") continue;
      const v = versions[o];
      if (v) out[o] = defaultCoreRange(v);
    }
    return out;
  }, [named, owner, versions, name]);
  const text = useMemo(
    () => manifest(name || "My module", modelName || "Record", named, st, core, needs),
    [name, modelName, named, st, core, needs],
  );
  const id = slug(name);
  const upd = (i: number, patch: Partial<DField>) =>
    setFields(fields.map((f, j) => (j === i ? { ...f, ...patch } : f)));

  function problem(): string | null {
    if (!id) return t("Give the module a name");
    if (named.length === 0 && st.length < 2) return t("Add at least one field");
    const names = named.map(fieldName);
    const dup = names.find((n, i) => names.indexOf(n) !== i);
    if (dup) return t("Two fields are called {name}", { name: dup });
    if (names.includes("id")) return t("A field cannot be called id");
    for (const f of named) {
      if (f.kind === "select" && listOf(f.options).length === 0)
        return t("Choices for {name} need at least one option", { name: fieldName(f) });
      if (f.kind === "ref" && !f.to)
        return t("Pick the record type that {name} links to", { name: fieldName(f) });
      if (f.kind === "decimal") {
        const p = Number.parseInt(f.precision, 10);
        const s = Number.parseInt(f.scale, 10);
        if (!(p >= 1 && p <= 38) || !(s >= 0 && s <= 12 && s <= p))
          return t("{name} needs 1 to 38 digits, of which 0 to 12 after the point", {
            name: fieldName(f),
          });
      }
    }
    return null;
  }

  return (
    <div className="two">
      <div className="stack">
        <p className="hint">
          {t(
            "Describe a model and install it as a module. It gets the API, agent tools, permissions and privacy rules like every other app.",
          )}{" "}
          <a className="reflink" href={href({ app: "customize", model: "guide", tab: "guide" })}>
            {t("How to write a custom module?")}
          </a>
        </p>
        <label className="field">
          <span className="flabel">{t("Module name")}</span>
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="field">
          <span className="flabel">{t("Record type")}</span>
          <input value={modelName} onChange={(e) => setModelName(e.target.value)} />
        </label>
        <label className="field">
          <span className="flabel">{t("Core versions it works with")}</span>
          <input
            value={core}
            onChange={(e) => setCore(e.target.value)}
            aria-label={t("Core versions it works with")}
          />
          <span className="hint">
            {t(
              "A range such as ^0.2 or >=0.2, <0.4. This instance runs core {version}. Every module must say which versions it supports; an update is refused when it would leave a range.",
              { version: getCoreVersion() || "?" },
            )}
          </span>
        </label>
        {Object.keys(needs).length > 0 && (
          <p className="hint">
            {t("It needs {modules}, so the ranges are written into obop.toml.", {
              modules: Object.entries(needs)
                .map(([m, r]) => `${m} ${r}`)
                .join(", "),
            })}
          </p>
        )}
        <h3>{t("Fields")}</h3>
        <ul className="rows">
          {fields.map((f, i) => (
            <li key={i} className="row wrap fieldrow">
              <div className="frow">
                <input
                  value={f.name}
                  onChange={(e) => upd(i, { name: e.target.value })}
                  placeholder={t("name")}
                  aria-label={t("Field name")}
                />
                <select
                  value={f.kind}
                  onChange={(e) => upd(i, { kind: e.target.value as Kind })}
                  aria-label={t("Field kind")}
                >
                  {KINDS.map(([k, l]) => (
                    <option key={k} value={k}>
                      {t(l)}
                    </option>
                  ))}
                </select>
                <button
                  className="icon-btn"
                  onClick={() => setFields(fields.filter((_, j) => j !== i))}
                  aria-label={t("Remove field")}
                >
                  ×
                </button>
              </div>
              {f.kind === "select" && (
                <input
                  value={f.options}
                  onChange={(e) => upd(i, { options: e.target.value })}
                  placeholder={t("options, comma separated")}
                  aria-label={t("Options")}
                />
              )}
              {f.kind === "decimal" && (
                <div className="frow">
                  <input
                    type="number"
                    min={1}
                    max={38}
                    value={f.precision}
                    onChange={(e) => upd(i, { precision: e.target.value })}
                    aria-label={t("Digits in all")}
                    title={t("Digits in all")}
                  />
                  <input
                    type="number"
                    min={0}
                    max={12}
                    value={f.scale}
                    onChange={(e) => upd(i, { scale: e.target.value })}
                    aria-label={t("Digits after the point")}
                    title={t("Digits after the point")}
                  />
                  <select
                    value={f.round}
                    onChange={(e) => upd(i, { round: e.target.value })}
                    aria-label={t("Rounding when importing")}
                    title={t("Rounding when importing")}
                  >
                    <option value="">{t("Refuse extra digits")}</option>
                    <option value="half_even">{t("Round half to even")}</option>
                    <option value="half_up">{t("Round half up")}</option>
                    <option value="down">{t("Drop extra digits")}</option>
                  </select>
                </div>
              )}
              {f.kind === "ref" && (
                <select
                  value={f.to}
                  onChange={(e) => upd(i, { to: e.target.value })}
                  aria-label={t("Target model")}
                >
                  <option value="">{t("Links to…")}</option>
                  {targets.map((x) => (
                    <option key={x.value} value={x.value}>
                      {x.text}
                    </option>
                  ))}
                </select>
              )}
              <div className="frow">
                <input
                  value={f.description}
                  onChange={(e) => upd(i, { description: e.target.value })}
                  placeholder={t("description")}
                  aria-label={t("Field description")}
                />
                <select
                  value={f.class}
                  onChange={(e) => upd(i, { class: e.target.value })}
                  aria-label={t("Data class")}
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
                  {t("Required")}
                </label>
                <label className="check">
                  <input
                    type="checkbox"
                    checked={f.personal}
                    onChange={(e) => upd(i, { personal: e.target.checked })}
                  />
                  {t("Personal data")}
                </label>
                {fieldName(f) !== slug(f.name) && (
                  <span className="hint">{t("stored as {name}", { name: fieldName(f) })}</span>
                )}
              </div>
            </li>
          ))}
        </ul>
        <button className="btn" onClick={() => setFields([...fields, blank()])}>
          {t("Add field")}
        </button>
        <label className="field">
          <span className="flabel">{t("Status field (optional, statuses comma separated)")}</span>
          <input
            value={statuses}
            onChange={(e) => setStatuses(e.target.value)}
            placeholder={t("open, in progress, done")}
          />
          <span className="hint">
            {t(
              "Adds a choice field called state, one action per step and a status bar on the form.",
            )}
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
                notify(t("Installed {name}", { name }));
                onModulesChanged();
                location.hash = href({ app: id });
              } else
                setError(
                  r.kind === "error"
                    ? serverMessage(r.message)
                    : t("Waiting for a human to approve"),
                );
            }}
          >
            {t("Create and install")}
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
