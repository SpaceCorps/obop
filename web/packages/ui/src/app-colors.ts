import { oklchToHex } from "./colors.ts";
import { APP_ICONS, type AppIconName } from "./icon-data.ts";

// Which glyph and which tile colour stand for an app. Pure data and functions, no React, so the
// static landing page and the React apps agree.

/** Module id (and a few spellings of it) to the glyph that stands for it. */
const ALIASES: Record<string, AppIconName> = {
  base: "contacts",
  party: "contacts",
  product: "products",
  projects: "project",
  fieldservice: "field-service",
  field_service: "field-service",
  pointofsale: "pos",
  point_of_sale: "pos",
  shop: "ecommerce",
  email_marketing: "email-marketing",
  emailmarketing: "email-marketing",
  mass_mailing: "email-marketing",
  sign: "sign",
  esign: "sign",
  platform: "customize",
  timesheet: "timesheets",
  expense: "expenses",
  appointment: "appointments",
  subscription: "subscriptions",
  dashboard: "dashboards",
  document: "documents",
  kb: "knowledge",
  wiki: "knowledge",
};

/** The icon for an app, from its module id first and its name second; unknown apps get the generic one. */
export function appIconName(id: string, name = ""): AppIconName {
  const key = id.toLowerCase();
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  for (const k of [key, slug]) {
    if (k in APP_ICONS && k !== "app") return k as AppIconName;
    const a = ALIASES[k.replace(/-/g, "_")] ?? ALIASES[k.replace(/-/g, "")];
    if (a) return a;
  }
  return "app";
}

/** Brand tile colours as OKLCH: lightness 0.52 to 0.6 keeps the white glyph above 3:1 everywhere. */
const TILE: Record<string, [number, number, number]> = {
  crm: [0.55, 0.17, 258],
  sales: [0.56, 0.14, 152],
  invoicing: [0.56, 0.11, 200],
  products: [0.6, 0.15, 55],
  contacts: [0.54, 0.17, 305],
  project: [0.55, 0.17, 278],
  helpdesk: [0.58, 0.18, 25],
  fleet: [0.55, 0.12, 235],
  inventory: [0.56, 0.1, 78],
  purchase: [0.56, 0.17, 345],
  manufacturing: [0.58, 0.15, 38],
  hr: [0.56, 0.16, 355],
  timesheets: [0.56, 0.1, 188],
  planning: [0.54, 0.16, 288],
  "field-service": [0.58, 0.13, 68],
  pos: [0.56, 0.13, 165],
  ecommerce: [0.56, 0.18, 330],
  website: [0.55, 0.14, 225],
  "email-marketing": [0.58, 0.17, 12],
  documents: [0.52, 0.07, 250],
  sign: [0.54, 0.18, 295],
  subscriptions: [0.56, 0.11, 175],
  dashboards: [0.52, 0.15, 268],
  accounting: [0.55, 0.13, 142],
  knowledge: [0.57, 0.11, 118],
  appointments: [0.55, 0.13, 215],
  expenses: [0.58, 0.12, 92],
  connectors: [0.53, 0.12, 192],
  customize: [0.5, 0.04, 265],
  app: [0.5, 0.04, 265],
};

/** The default tile colour (hex) for an app. */
export function appColor(id: string, name = "", chosen?: string): string {
  if (chosen) return chosen;
  const icon = appIconName(id, name);
  if (icon !== "app") {
    const [l, c, h] = TILE[icon]!;
    return oklchToHex({ l, c, h });
  }
  // Unknown modules get a stable hue from their id.
  let n = 0;
  for (const ch of id) n = (n * 31 + ch.charCodeAt(0)) >>> 0;
  return oklchToHex({ l: 0.55, c: 0.15, h: n % 360 });
}
