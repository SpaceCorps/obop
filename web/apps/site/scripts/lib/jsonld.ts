import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { messages, type Locale } from "../../i18n/index.ts";
import { GEN, GITHUB, SITE_URL } from "./paths.ts";

interface Rec {
  locale: string;
  path: string;
  url: string;
  title: string;
}

let cache: Rec[] | undefined;
function pages(): Rec[] {
  if (!cache) {
    const f = join(GEN, "pages.json");
    cache = existsSync(f) ? (JSON.parse(readFileSync(f, "utf8")) as Rec[]) : [];
  }
  return cache;
}

interface Ctx {
  fm: Record<string, unknown>;
  locale: Locale;
  path: string;
  canonical: string;
  title: string;
  description: string;
  githubBlob: string;
}

const ORG = {
  "@type": "Organization",
  "@id": `${SITE_URL}/#organization`,
  name: "obop",
  url: `${SITE_URL}/`,
  logo: `${SITE_URL}/apple-touch-icon.png`,
  sameAs: [GITHUB],
  // No mailbox exists yet: people reach the project through GitHub.
  contactPoint: [
    {
      "@type": "ContactPoint",
      contactType: "customer support",
      url: `${GITHUB}/issues`,
      availableLanguage: ["en", "de", "fr", "pl", "fi", "sv", "es"],
    },
    {
      "@type": "ContactPoint",
      contactType: "security",
      url: `${GITHUB}/security/advisories/new`,
    },
  ],
};

/** Structured data for one page: Organization and WebSite on the home page, breadcrumbs, and the type that fits. */
export function jsonLd(c: Ctx): object[] {
  const m = messages[c.locale];
  const lang = m.htmlLang;
  const out: object[] = [];
  const ctx = "https://schema.org";
  const section = String(c.fm["section"] ?? "");

  if (c.path === "/") {
    out.push({ "@context": ctx, ...ORG });
    out.push({
      "@context": ctx,
      "@type": "WebSite",
      "@id": `${SITE_URL}/#website`,
      url: `${SITE_URL}/`,
      name: "obop",
      description: m.description,
      inLanguage: lang,
      publisher: { "@id": `${SITE_URL}/#organization` },
      potentialAction: {
        "@type": "SearchAction",
        target: {
          "@type": "EntryPoint",
          urlTemplate: `${SITE_URL}/search/?q={search_term_string}`,
        },
        "query-input": "required name=search_term_string",
      },
    });
    out.push({
      "@context": ctx,
      "@type": "SoftwareApplication",
      name: "obop",
      description: m.description,
      url: `${SITE_URL}/`,
      applicationCategory: "BusinessApplication",
      operatingSystem: "Web, Linux, macOS",
      inLanguage: lang,
      codeRepository: GITHUB,
      offers: {
        "@type": "Offer",
        price: "0",
        priceCurrency: "EUR",
        description: "Self-hosted open-source edition",
      },
    });
    out.push(faq(m.home.faq));
  } else {
    const parts = c.path.split("/").filter(Boolean);
    const items: object[] = [
      {
        "@type": "ListItem",
        position: 1,
        name: "obop",
        item: `${SITE_URL}${c.locale === "en" ? "" : `/${c.locale}`}/`,
      },
    ];
    let acc = "";
    parts.forEach((p, i) => {
      acc += `/${p}`;
      const rec = pages().find((r) => r.locale === c.locale && r.path === `${acc}/`);
      items.push({
        "@type": "ListItem",
        position: i + 2,
        name: rec?.title ?? p,
        item: `${SITE_URL}${c.locale === "en" ? "" : `/${c.locale}`}${acc}/`,
      });
    });
    out.push({ "@context": ctx, "@type": "BreadcrumbList", itemListElement: items });
  }

  if (c.path === "/pricing/") out.push(faq(m.pricing.faq));
  if (section === "apps" && c.fm["appId"]) {
    out.push({
      "@context": ctx,
      "@type": "SoftwareApplication",
      name: `${String(c.fm["title"])} (obop app)`,
      description: c.description,
      url: c.canonical,
      applicationCategory: "BusinessApplication",
      operatingSystem: "Web",
      inLanguage: lang,
      softwareVersion: c.fm["builtin"] ? undefined : String(c.fm["version"]),
      softwareRequirements: "obop",
      isPartOf: { "@id": `${SITE_URL}/#website` },
      offers: {
        "@type": "Offer",
        price: "0",
        priceCurrency: "EUR",
        description: "Included in the open-source edition",
      },
    });
  }
  if (section === "docs") {
    out.push({
      "@context": ctx,
      "@type": "TechArticle",
      headline: c.title.replace(/ \| obop$/, ""),
      description: c.description,
      inLanguage: c.fm["fallback"] === true ? "en" : lang,
      url: c.canonical,
      mainEntityOfPage: c.canonical,
      author: { "@id": `${SITE_URL}/#organization` },
      publisher: { "@type": "Organization", name: "obop", url: `${SITE_URL}/` },
    });
  }
  return out;
}

function faq(items: { q: string; a: string }[]): object {
  return {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    mainEntity: items.map((i) => ({
      "@type": "Question",
      name: i.q,
      acceptedAnswer: { "@type": "Answer", text: i.a },
    })),
  };
}
