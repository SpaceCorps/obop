import type { Category } from "../data/apps.ts";

export const LOCALES = ["en", "de", "fr", "pl", "fi", "sv", "es"] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = "en";

export interface Link {
  label: string;
  /** Site path without locale prefix, for example "/apps/". */
  href: string;
}

export interface Plan {
  name: string;
  price: string;
  period: string;
  blurb: string;
  features: string[];
  highlight?: boolean;
}

/** Everything on the site that is not Markdown prose: chrome, home, pricing, apps strings. */
export interface Messages {
  htmlLang: string;
  /** Name of the language in that language. */
  langName: string;
  ogLocale: string;
  tagline: string;
  description: string;
  ui: {
    skip: string;
    menu: string;
    mainNav: string;
    footerNav: string;
    docsNav: string;
    breadcrumb: string;
    home: string;
    signIn: string;
    startFree: string;
    search: string;
    searchPlaceholder: string;
    searchEmpty: string;
    searchResultsFor: string;
    searchNeedsJs: string;
    theme: string;
    language: string;
    onThisPage: string;
    editPage: string;
    prev: string;
    next: string;
    copy: string;
    copied: string;
    markdownVersion: string;
    notTranslatedTitle: string;
    notTranslatedBody: string;
    notTranslatedLink: string;
    langSuggest: string;
    langSuggestGo: string;
    dismiss: string;
    betaNote: string;
    skipToContent: string;
    allRightsNote: string;
    sourceCode: string;
    docsHome: string;
  };
  nav: Link[];
  footer: { groups: { title: string; links: Link[] }[] };
  home: {
    eyebrow: string;
    badge: string;
    titleA: string;
    titleB: string;
    lead: string;
    ctaPrimary: string;
    ctaSecondary: string;
    heroPoints: string[];
    mockCaption: string;
    appsTitle: string;
    appsSub: string;
    seeAllApps: string;
    pillarsTitle: string;
    pillarsSub: string;
    pillars: { icon: string; title: string; body: string; tag: string }[];
    flowLabel: string;
    flow: { title: string; body: string }[];
    marketTitle: string;
    marketBody: string;
    marketTicks: string[];
    marketCta: string;
    stepsTitle: string;
    steps: { title: string; body: string }[];
    stepsCta: string;
    openTitle: string;
    openBody: string;
    openPoints: { title: string; body: string }[];
    pricingTitle: string;
    pricingSub: string;
    pricingFine: string;
    pricingCta: string;
    faqTitle: string;
    faq: { q: string; a: string }[];
    ctaTitle: string;
    ctaBody: string;
    ctaButton: string;
  };
  pricing: {
    title: string;
    description: string;
    lead: string;
    plans: Plan[];
    plannedBadge: string;
    note: string;
    selfHostTitle: string;
    selfHostBody: string;
    faqTitle: string;
    faq: { q: string; a: string }[];
    cta: string;
  };
  apps: {
    title: string;
    description: string;
    lead: string;
    filterLabel: string;
    all: string;
    available: string;
    marketplace: string;
    categories: Record<Category, string>;
    status: { available: string; marketplace: string };
    countLine: string;
    h: {
      purpose: string;
      package: string;
      models: string;
      fields: string;
      workflows: string;
      permissions: string;
      approvals: string;
      agents: string;
      details: string;
      dependencies: string;
      install: string;
      guide: string;
      related: string;
    };
    th: {
      field: string;
      kind: string;
      dataClass: string;
      personal: string;
      group: string;
      forAgents: string;
      action: string;
      moves: string;
      who: string;
      approval: string;
      model: string;
      readGroup: string;
      writeGroup: string;
      description: string;
    };
    yes: string;
    no: string;
    clear: string;
    masked: string;
    withheld: string;
    noWorkflow: string;
    noApproval: string;
    noDependencies: string;
    depLead: string;
    depInstalled: string;
    meta: {
      version: string;
      builtinVersion: string;
      author: string;
      license: string;
      core: string;
      coreNow: string;
      maturity: string;
      maturities: Record<"stable" | "beta" | "experimental", string>;
      categories: string;
      categoryNames: Record<string, string>;
      builtinWord: string;
    };
    installBuiltin: string;
    installPackage: string;
    installNote: string;
    guideLink: string;
    englishNote: string;
    agentsCan: string;
    agentsPropose: string;
    agentsMasked: string;
    agentsWithheld: string;
    agentsNone: string;
    approvalsLead: string;
    permissionsLead: string;
    versionLabel: string;
    /** One-line taglines by module id. */
    taglines: Record<string, string>;
  };
  docs: { groups: { start: string; concepts: string; agents: string; reference: string } };
  notFound: { title: string; body: string; searchHint: string; links: string; english: string };
  search: { title: string; description: string };
}
