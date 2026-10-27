import type { Messages } from "../../i18n/types.ts";
import type { App } from "./apps.ts";

/** Plain Markdown equivalents of the pages that are drawn from data (home, apps index, pricing). */
export function homeMirror(m: Messages, prefix: string, apps: App[]): string {
  const h = m.home;
  const out: string[] = [];
  out.push(`**${h.titleA} ${h.titleB}**`, "", h.lead, "");
  out.push(`- [${h.ctaPrimary}](/app/)`, `- [${h.ctaSecondary}](${prefix}/apps/)`, "");
  out.push(`## ${h.appsTitle}`, "", h.appsSub, "");
  for (const a of apps) {
    out.push(
      `- [${a.name}](${prefix}/apps/${a.slug}/): ${m.apps.taglines[a.id] ?? a.tagline} (${a.builtin ? m.apps.status.available : m.apps.status.marketplace})`,
    );
  }
  out.push("", `## ${h.pillarsTitle}`, "", h.pillarsSub, "");
  for (const p of h.pillars) out.push(`### ${p.title}`, "", p.body, "", `*${p.tag}*`, "");
  out.push(`### ${h.flowLabel}`, "");
  h.flow.forEach((f, i) => out.push(`${i + 1}. **${f.title}.** ${f.body}`));
  out.push("", `## ${h.marketTitle}`, "", h.marketBody, "");
  for (const t of h.marketTicks) out.push(`- ${t}`);
  out.push("", `## ${h.stepsTitle}`, "");
  h.steps.forEach((s, i) => out.push(`${i + 1}. **${s.title}.** ${s.body}`));
  out.push("", `## ${h.openTitle}`, "", h.openBody, "");
  for (const p of h.openPoints) out.push(`- **${p.title}.** ${p.body}`);
  out.push("", `## ${h.pricingTitle}`, "", h.pricingSub, "");
  for (const p of m.pricing.plans.slice(0, 4))
    out.push(`- **${p.name}**: ${p.price}${p.period}. ${p.blurb}`);
  out.push("", `*${h.pricingFine}*`, "", `[${h.pricingCta}](${prefix}/pricing/)`, "");
  out.push(`## ${h.faqTitle}`, "");
  for (const f of h.faq) out.push(`### ${f.q}`, "", f.a, "");
  out.push(`## ${h.ctaTitle}`, "", h.ctaBody, "", `[${h.ctaButton}](/app/)`, "");
  return out.join("\n");
}

export function appsIndexMirror(m: Messages, prefix: string, apps: App[]): string {
  const out: string[] = [m.apps.lead, ""];
  const cats = Object.keys(m.apps.categories) as (keyof Messages["apps"]["categories"])[];
  for (const c of cats) {
    const list = apps.filter((a) => a.category === c);
    if (list.length === 0) continue;
    out.push(`## ${m.apps.categories[c]}`, "");
    for (const a of list) {
      out.push(
        `- [${a.name}](${prefix}/apps/${a.slug}/): ${m.apps.taglines[a.id] ?? a.tagline} (${a.builtin ? m.apps.status.available : m.apps.status.marketplace})`,
      );
    }
    out.push("");
  }
  return out.join("\n");
}

export function pricingMirror(m: Messages): string {
  const p = m.pricing;
  const out: string[] = [p.lead, "", `*${p.plannedBadge}*`, ""];
  for (const plan of p.plans) {
    out.push(`## ${plan.name}: ${plan.price}${plan.period}`, "", plan.blurb, "");
    for (const f of plan.features) out.push(`- ${f}`);
    out.push("");
  }
  out.push(
    p.note,
    "",
    `## ${p.selfHostTitle}`,
    "",
    p.selfHostBody,
    "",
    `## ${p.hostingTitle}`,
    "",
    p.hostingBody,
    "",
    `## ${p.faqTitle}`,
    "",
  );
  for (const f of p.faq) out.push(`### ${f.q}`, "", f.a, "");
  return out.join("\n");
}
