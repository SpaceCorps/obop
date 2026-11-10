import type { Messages } from "../../i18n/types.ts";
import { agentView, appName, type App } from "./apps.ts";
import { appView, lowerFirst } from "./app-view.ts";
import { CORE_VERSION } from "./paths.ts";

const esc = (s: string) => s.replace(/\|/g, "\\|");
const code = (s: string) => `\`${s}\``;
const fill = (s: string, vars: Record<string, string>) =>
  s.replace(/\{(\w+)\}/g, (_m, k: string) => vars[k] ?? "");

/** English text inside a translated page is marked with lang="en" for screen readers. */
export function english(text: string, locale: string, m: Messages): string {
  if (locale === "en") return text;
  return `<div lang="en" class="en-note" data-label="${m.apps.englishNote}">\n\n${text}\n\n</div>`;
}

/**
 * The technical Markdown of one app page, which the page shows under "For developers": the package
 * text, module details, models with their field tables, workflows, permission groups, approval steps,
 * the agent view, dependencies, install and the link to the module guide. No H1 and no purpose (the
 * layout draws the plain-words layer above it). Heading texts, and so their ids, are those the
 * whole page had before it was split, so links to them keep working. The headings are one level
 * below the page's "For developers" heading (h3, h4), on the page and in the Markdown copy alike.
 * `mirror` produces the plain-Markdown variant for the .md copy and llms-full.txt.
 */
export function developerBody(
  app: App,
  locale: string,
  m: Messages,
  mirror: boolean,
  prefix: string,
  all: App[] = [],
): string {
  const a = m.apps;
  const eng = (t: string) => (mirror ? t : english(t, locale, m));
  const out: string[] = [];
  const h2 = "###";
  const h3 = "####";
  // A field description is English, from the manifest: on a translated page it is marked as such.
  const desc = (t: string) =>
    mirror || locale === "en" ? esc(t) : `<span lang="en">${esc(t)}</span>`;

  out.push(`${h2} ${a.h.package}`, "", eng(app.description), "");
  // What the plain text above leaves out as too technical (an API, a hash, a data class).
  if (app.technical) out.push(eng(app.technical), "");

  // Version, author, licence, core requirement, maturity: what the manifest declares (D55).
  const meta = app.meta;
  const spdx = /^[A-Za-z0-9.+-]+$/.test(meta.license)
    ? `[${meta.license}](https://spdx.org/licenses/${meta.license}.html)`
    : meta.license;
  const rows: [string, string][] = [
    [
      a.meta.version,
      app.builtin ? fill(a.meta.builtinVersion, { version: code(app.version) }) : code(app.version),
    ],
    [a.meta.author, esc([meta.author, ...meta.authors].filter(Boolean).join(", "))],
    [a.meta.license, spdx],
    [
      a.meta.core,
      `${code(meta.requiresCore)} (${fill(a.meta.coreNow, { version: CORE_VERSION })})`,
    ],
    [
      a.meta.maturity,
      a.meta.maturities[meta.maturity as keyof typeof a.meta.maturities] ?? meta.maturity,
    ],
    [a.meta.categories, meta.categories.map((c) => a.meta.categoryNames[c] ?? c).join(", ")],
  ].filter((r): r is [string, string] => !!r[1]);
  out.push(`${h2} ${a.h.details}`, "");
  for (const [k, v] of rows) out.push(`- **${k}:** ${v}`);
  out.push("");

  out.push(`${h2} ${a.h.models}`, "");
  for (const model of app.models) {
    out.push(`${h3} ${code(model.name)}`, "", eng(model.description), "");
    out.push(
      `${a.th.readGroup}: ${code(model.read_group)}. ${a.th.writeGroup}: ${code(model.write_group)}.`,
      "",
    );
    out.push(
      `| ${a.th.field} | ${a.th.kind} | ${a.th.dataClass} | ${a.th.personal} | ${a.th.group} | ${a.th.forAgents} | ${a.th.description} |`,
      "|---|---|---|---|---|---|---|",
    );
    for (const f of model.fields) {
      const kind =
        f.kind === "select" && f.options
          ? `select (${f.options.join(", ")})`
          : f.kind === "ref"
            ? `ref → ${f.to}`
            : f.kind;
      const view = agentView(f);
      out.push(
        `| ${code(f.name)} | ${esc(kind)} | ${code(f.class)} | ${f.personal_data ? a.yes : a.no} | ${code(f.group)} | ${a[view]} | ${desc(f.description)} |`,
      );
    }
    out.push("");
  }

  out.push(`${h2} ${a.h.workflows}`, "");
  const withActions = app.models.filter((x) => x.actions.length > 0);
  if (withActions.length === 0) out.push(a.noWorkflow, "");
  for (const model of withActions) {
    out.push(`${h3} ${code(model.name)}`, "");
    out.push(
      `| ${a.th.action} | ${a.th.moves} | ${a.th.who} | ${a.th.approval} |`,
      "|---|---|---|---|",
    );
    for (const act of model.actions) {
      const moves = act.field ? `${act.field}: ${act.from ?? "*"} → ${act.to}` : "-";
      out.push(
        `| ${code(`${model.name}.${act.name}`)} | ${esc(moves)} | ${code(act.group)} | ${act.approval ? a.yes : a.no} |`,
      );
    }
    out.push("");
  }

  const groups = new Set<string>();
  for (const model of app.models) {
    groups.add(model.read_group);
    groups.add(model.write_group);
    for (const f of model.fields) groups.add(f.group);
    for (const act of model.actions) groups.add(act.group);
  }
  out.push(`${h2} ${a.h.permissions}`, "", a.permissionsLead, "");
  for (const g of [...groups].sort()) out.push(`- ${code(g)}`);
  out.push("");

  const approvals = app.models.flatMap((model) =>
    model.actions.filter((x) => x.approval).map((x) => `${model.name}.${x.name}`),
  );
  out.push(`${h2} ${a.h.approvals}`, "");
  if (approvals.length === 0) out.push(a.noApproval, "");
  else {
    out.push(a.approvalsLead, "");
    for (const x of approvals) out.push(`- ${code(x)}`);
    out.push("");
  }

  const masked = app.models.flatMap((model) =>
    model.fields
      .filter((f) => agentView(f) === "masked")
      .map((f) => code(`${model.name}.${f.name}`)),
  );
  const withheld = app.models.flatMap((model) =>
    model.fields
      .filter((f) => agentView(f) === "withheld")
      .map((f) => code(`${model.name}.${f.name}`)),
  );
  out.push(`${h2} ${a.h.agents}`, "", a.agentsCan, "");
  const propose = [...approvals.map(code), ...app.models.map((x) => code(`${x.name}.delete`))];
  out.push(fill(a.agentsPropose, { list: propose.join(", ") }), "");
  if (masked.length) out.push(fill(a.agentsMasked, { list: masked.join(", ") }), "");
  if (withheld.length) out.push(fill(a.agentsWithheld, { list: withheld.join(", ") }), "");

  out.push(`${h2} ${a.h.dependencies}`, "");
  if (app.dependencies.length === 0) out.push(a.noDependencies, "");
  else {
    out.push(a.depLead, "");
    for (const d of app.dependencies) {
      const dep = all.find((x) => x.id === d.id);
      const name = dep ? `[${appName(dep, locale)}](${prefix}/apps/${dep.slug}/)` : code(d.id);
      out.push(
        `- ${name}: ${code(d.requirement)}${dep?.builtin ? ` (${a.meta.builtinWord})` : ""}`,
      );
    }
    out.push("");
    if (!app.builtin) out.push(a.depInstalled, "");
  }

  out.push(`${h2} ${a.h.install}`, "");
  if (app.builtin) out.push(a.installBuiltin, "");
  else {
    out.push(
      a.installPackage,
      "",
      "```bash",
      `obop modules install-from ${app.id}`,
      "```",
      "",
      a.installNote,
      "",
    );
  }
  out.push(
    `${h2} ${a.h.guide}`,
    "",
    `[${a.guideLink}](${prefix}/docs/reference/module-manifest/)`,
    "",
  );
  return out.join("\n");
}

/**
 * The plain-Markdown copy of an app page (the .md mirror, llms-full.txt and the search index): the
 * same sections in the same order as the page, from the same view model, then the technical part
 * under "For developers".
 */
export function appMirror(
  app: App,
  locale: string,
  m: Messages,
  prefix: string,
  all: App[],
): string {
  const a = m.apps;
  const s = a.store;
  const v = appView(app, m, all, locale);
  const out: string[] = [a.taglines[app.id] ?? app.tagline, "", v.category, ""];
  for (const f of v.facts) out.push(`- **${f.label}:** ${f.value}`);
  out.push("");
  if (v.experimental) out.push(s.experimentalNote, "");
  if (app.builtin) out.push(`[${s.open}](/app/)`, "");

  // The page shows the purpose in two parts (large, then quiet); the copy says it in one paragraph.
  out.push(`## ${a.h.purpose}`, "");
  if (locale !== "en") out.push(`*${s.purposeEnglish}*`, "");
  out.push([v.purposeLead, v.purposeRest].filter(Boolean).join(" "), "");

  out.push(`## ${s.canDo}`, "");
  if (v.actions.length === 0) out.push(s.directEdit, "");
  else {
    for (const c of v.actions) {
      const note = c.approval ? ` (${s.needsApproval})` : "";
      const title = locale === "en" ? lowerFirst(c.title) : c.title;
      out.push(`- **${c.kicker}: ${title}**${note}: ${c.text}`);
    }
    out.push("");
  }

  out.push(`## ${s.keeps}`, "");
  for (const c of v.models) out.push(`- **${c.title}**: ${c.text}`);
  out.push("");

  out.push(`## ${s.safe}`, "");
  for (const line of v.safe) out.push(`- ${line}`);
  out.push("");

  if (v.worksWith.length > 0) {
    out.push(`## ${s.worksWith}`, "");
    for (const d of v.worksWith) out.push(`- [${d.name}](${prefix}/apps/${d.slug}/)`);
    out.push("");
  }

  out.push(`## ${s.info}`, "");
  for (const r of v.info) {
    const value = r.href ? `[${r.value}](${r.href})` : r.mono ? code(r.value) : r.value;
    out.push(`- **${r.label}:** ${value}${r.note ? ` ${r.note}` : ""}`);
  }
  out.push("");

  // A built-in app says on one quiet line that it is included; a marketplace app has a Get section.
  if (v.command === null) out.push(s.included, "");
  else
    out.push(
      `## ${s.get}`,
      "",
      s.getLead,
      "",
      `[${s.openObop}](/app/)`,
      "",
      `${s.commandLine}:`,
      "",
      "```bash",
      v.command,
      "```",
      "",
      s.getNote,
      "",
    );

  out.push(`## ${s.developers}`, "", developerBody(app, locale, m, true, prefix, all));

  if (v.related.length > 0) {
    out.push(`## ${a.h.related}`, "");
    for (const d of v.related) out.push(`- [${d.name}](${prefix}/apps/${d.slug}/)`);
    out.push("");
  }
  return out.join("\n");
}
