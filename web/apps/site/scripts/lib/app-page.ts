import type { Messages } from "../../i18n/types.ts";
import { agentView, type App } from "./apps.ts";

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
 * The Markdown body of one app page (no H1; the layout draws the title). `mirror` produces the
 * plain-Markdown variant for the .md copy and llms-full.txt.
 */
export function appBody(
  app: App,
  locale: string,
  m: Messages,
  mirror: boolean,
  prefix: string,
): string {
  const a = m.apps;
  const eng = (t: string) => (mirror ? t : english(t, locale, m));
  const out: string[] = [];

  out.push(`## ${a.h.purpose}`, "", eng(app.purpose), "");
  out.push(`## ${a.h.package}`, "", eng(app.description), "");

  out.push(`## ${a.h.models}`, "");
  for (const model of app.models) {
    out.push(`### ${code(model.name)}`, "", eng(model.description), "");
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
        `| ${code(f.name)} | ${esc(kind)} | ${code(f.class)} | ${f.personal_data ? a.yes : a.no} | ${code(f.group)} | ${a[view]} | ${esc(f.description)} |`,
      );
    }
    out.push("");
  }

  out.push(`## ${a.h.workflows}`, "");
  const withActions = app.models.filter((x) => x.actions.length > 0);
  if (withActions.length === 0) out.push(a.noWorkflow, "");
  for (const model of withActions) {
    out.push(`### ${code(model.name)}`, "");
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
  out.push(`## ${a.h.permissions}`, "", a.permissionsLead, "");
  for (const g of [...groups].sort()) out.push(`- ${code(g)}`);
  out.push("");

  const approvals = app.models.flatMap((model) =>
    model.actions.filter((x) => x.approval).map((x) => `${model.name}.${x.name}`),
  );
  out.push(`## ${a.h.approvals}`, "");
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
  out.push(`## ${a.h.agents}`, "", a.agentsCan, "");
  const propose = [...approvals.map(code), ...app.models.map((x) => code(`${x.name}.delete`))];
  out.push(fill(a.agentsPropose, { list: propose.join(", ") }), "");
  if (masked.length) out.push(fill(a.agentsMasked, { list: masked.join(", ") }), "");
  if (withheld.length) out.push(fill(a.agentsWithheld, { list: withheld.join(", ") }), "");

  out.push(`## ${a.h.dependencies}`, "");
  out.push(
    app.depends.length ? app.depends.map((d) => `- ${code(d)}`).join("\n") : a.noDependencies,
    "",
  );

  out.push(`## ${a.h.install}`, "");
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
    `## ${a.h.guide}`,
    "",
    `[${a.guideLink}](${prefix}/docs/reference/module-manifest/)`,
    "",
  );
  return out.join("\n");
}
