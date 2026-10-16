/** Order of the documentation sidebar. Group titles come from the messages (docs.groups). */
export const DOCS_NAV: { group: "start" | "concepts" | "agents" | "reference"; pages: string[] }[] = [
  { group: "start", pages: ["/docs/", "/docs/getting-started/"] },
  {
    group: "concepts",
    pages: [
      "/docs/concepts/",
      "/docs/concepts/apps-and-modules/",
      "/docs/concepts/instances/",
      "/docs/concepts/data-classes-and-masking/",
      "/docs/concepts/permission-groups/",
      "/docs/concepts/workflows-and-approvals/",
    ],
  },
  {
    group: "agents",
    pages: [
      "/docs/agents/",
      "/docs/agents/tokens/",
      "/docs/agents/mcp/",
      "/docs/agents/rest/",
      "/docs/agents/cli/",
      "/docs/agents/approvals/",
      "/docs/agents/masking/",
      "/docs/agents/safety/",
      "/docs/agents/workflows/",
      "/docs/agents/troubleshooting/",
    ],
  },
  {
    group: "reference",
    pages: [
      "/docs/reference/cli/",
      "/docs/reference/rest-api/",
      "/docs/reference/module-manifest/",
      "/docs/reference/environment/",
      "/docs/reference/self-hosting/",
      "/docs/reference/security/",
      "/docs/reference/faq/",
    ],
  },
];
