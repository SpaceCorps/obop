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
    hostingTitle: string;
    hostingBody: string;
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
    /**
     * The plain-words layer of the apps pages (the index and the top of every app page): short
     * labels, no jargon. The technical layer above (`h`, `th`, `meta`, ...) sits under "For developers".
     */
    store: {
      ready: string;
      more: string;
      open: string;
      get: string;
      canDo: string;
      keeps: string;
      safe: string;
      worksWith: string;
      info: string;
      developers: string;
      category: string;
      developer: string;
      /** The row that says which obop versions an app works with, and its value (uses {version}). */
      compat: string;
      compatValue: string;
      manifest: string;
      /** Where a built-in app's code lives (its manifest row is a snapshot, not the real manifest). */
      source: string;
      /** The fact for an app that needs no other app. */
      standalone: string;
      /**
       * The fact for an app that works with others: how many ({n}). One string per plural category
       * of the language (`Intl.PluralRules`), so Polish can say "2 aplikacje" and "5 aplikacji".
       */
      worksCount: Record<"one" | "few" | "many" | "other", string>;
      /** A built-in app's version in plain words (uses {version}): it moves with obop itself. */
      sameAsObop: string;
      directEdit: string;
      needsApproval: string;
      /** Uses {n}: the folded rest of the action cards and of the cards for what it keeps. */
      showMoreActions: string;
      showMoreKeeps: string;
      /**
       * The Get box of a marketplace app: one plain sentence, the button that opens obop (where
       * Customize and its Marketplace are), the label above the command line, and what it needs.
       */
      getLead: string;
      openObop: string;
      commandLine: string;
      getNote: string;
      /** A built-in app: the one quiet line under its information. */
      included: string;
      safeMasked: string;
      safeWithheld: string;
      safeApprove: string;
      safeApproveNone: string;
      /** The tail of the meta description of an app page in a language other than English. */
      appMeta: string;
      /** The label that opens the folded technical part under "For developers". */
      developersShow: string;
      /** The label of the same button while the part is open. */
      developersHide: string;
      /** Said under the purpose on a translated page, where the text is English. */
      purposeEnglish: string;
      /** The label that opens a long second paragraph under "What it is for". */
      readMore: string;
    };
    /** One-line taglines by module id. */
    taglines: Record<string, string>;
  };
  docs: { groups: { start: string; concepts: string; agents: string; reference: string } };
  notFound: { title: string; body: string; searchHint: string; links: string; english: string };
  search: { title: string; description: string };
}
