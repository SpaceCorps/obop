import { t } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";

/** The catalogue entry a `connector_provider` row describes. */
export type Provider = Row & {
  name: string;
  category: string;
  maturity: "working" | "generic_adapter" | "definition_only";
  engine: string;
  auth_kind: string;
  direction: "inbound" | "outbound" | "both";
  max_classes: string;
  purpose: string;
  description: string;
  legal_name: string;
  country: string;
  dpa_url: string;
  eu_adequate: "yes" | "no" | "verify";
  verify_note: string;
  allowed_hosts: string;
  config_fields: string;
  setup: string;
  webhook_scheme: string;
  oauth_scopes: string;
  enabled: boolean;
  users_may_use: boolean;
  dpa_recorded: boolean;
  legal_basis?: string;
  legal_basis_text: string;
};

export type ConfigField = {
  key: string;
  label: string;
  secret: boolean;
  required: boolean;
  help: string;
};

export type Connection = Row & {
  name: string;
  provider: string;
  owner_scope: string;
  owner_user: string;
  state: "draft" | "connected" | "paused" | "error" | "revoked";
  config: string;
  credentials_hint: string;
  field_allowlist: string;
  policy: string;
  retention_days: number;
  last_sync_at: string;
  last_error: string;
};

export type Consent = Row & {
  connection: string;
  state: "active" | "withdrawn" | "superseded";
  purpose: string;
  legal_basis: string;
  note: string;
  classes: string;
  fields_summary: string;
  recorded_at: string;
  withdrawn_at: string;
};

export type Run = Row & {
  connection: string;
  direction: string;
  kind: string;
  started_at: string;
  finished_at: string;
  status: string;
  records: number;
  rejected: number;
  proposed: number;
  error_summary: string;
};

export type Flow = Row & {
  connection: string;
  at: string;
  direction: string;
  model: string;
  record_count: number;
  fields_sent: string;
  fields_denied: string;
  masking_mode: string;
  outcome: string;
  reason: string;
};

export function parseJson<T>(text: unknown, fallback: T): T {
  if (typeof text !== "string" || !text.trim()) return fallback;
  try {
    return JSON.parse(text) as T;
  } catch {
    return fallback;
  }
}

export const configFields = (p: Provider): ConfigField[] => parseJson(p.config_fields, []);

/** The classes of CONVENTIONS section 5 that stay withheld until the policy is raised and acknowledged. */
export const HIGH_RISK = ["confidential_hr", "special_category", "financial"];

/** Models connectors never read or write: their own records and the platform's. */
export function blockedModel(name: string): boolean {
  return (
    name.startsWith("connector_") ||
    [
      "connection",
      "consent",
      "provider_policy",
      "sync_run",
      "data_flow_log",
      "processing_register",
      "egress_policy",
      "privacy_preview",
      "module",
      "setting",
      "marketplace_listing",
    ].includes(name)
  );
}

export function flowModels(models: Iterable<ModelMeta>): ModelMeta[] {
  return [...models].filter((m) => !blockedModel(m.name));
}

/** A field the person may put on an allowlist: readable and not a secret. */
export const listable = (f: FieldMeta) => f.class !== "secret";

export type Treatment = "allow" | "pseudonymise" | "deny";

/** What the default policy does with a field (the server decides; this is the same table, for the picker). */
export function defaultTreatment(f: FieldMeta, providerMax: string[]): Treatment {
  if (f.class === "secret") return "deny";
  if (!providerMax.includes(f.class)) return "deny";
  if (HIGH_RISK.includes(f.class)) return "deny";
  return f.personal_data ? "pseudonymise" : "allow";
}

export function treatmentLabel(tr: string, inbound: boolean): string {
  if (inbound) return tr === "deny" ? t("Never accepted") : t("Accepted");
  switch (tr) {
    case "deny":
      return t("Never sent");
    case "pseudonymise":
      return t("Sent as a pseudonym");
    case "mask":
      return t("Sent as a token");
    case "redact":
      return t("Sent redacted");
    default:
      return t("Sent as is");
  }
}

export const CLASS_LABEL: Record<string, () => string> = {
  none: () => t("Not personal"),
  public_directory: () => t("Directory data"),
  internal: () => t("Internal"),
  confidential_hr: () => t("Confidential HR"),
  special_category: () => t("Special category"),
  financial: () => t("Financial"),
  secret: () => t("Secret"),
};

export const className = (c: string) => (CLASS_LABEL[c] ? CLASS_LABEL[c]!() : c);

export function maturityLabel(m: Provider["maturity"]): string {
  return m === "working"
    ? t("Working")
    : m === "generic_adapter"
      ? t("Generic adapter")
      : t("Definition only");
}

export const maturityTone = (m: Provider["maturity"]) =>
  m === "working" ? "success" : m === "generic_adapter" ? "info" : "neutral";

export function directionLabel(d: string): string {
  return d === "inbound" ? t("Inbound") : d === "outbound" ? t("Outbound") : t("Both ways");
}

export function stateTone(s: string): "success" | "warning" | "danger" | "neutral" | "info" {
  return s === "connected"
    ? "success"
    : s === "paused" || s === "draft"
      ? "neutral"
      : s === "error"
        ? "danger"
        : s === "revoked"
          ? "warning"
          : "info";
}

export function stateLabel(s: string): string {
  switch (s) {
    case "draft":
      return t("Draft");
    case "connected":
      return t("Connected");
    case "paused":
      return t("Paused");
    case "error":
      return t("Error");
    case "revoked":
      return t("Revoked");
    default:
      return s;
  }
}

export function basisLabel(b: string): string {
  switch (b) {
    case "consent":
      return t("Consent");
    case "contract":
      return t("Contract");
    case "legitimate_interest":
      return t("Legitimate interest");
    case "legal_obligation":
      return t("Legal obligation");
    default:
      return b;
  }
}

export function adequacyLabel(a: Provider["eu_adequate"]): string {
  return a === "yes"
    ? t("EU or adequate country")
    : a === "no"
      ? t("Third country")
      : t("Verify with the provider");
}

/** Save a text as a file the person chooses where to keep. */
export function downloadText(filename: string, text: string, mime = "text/csv") {
  const url = URL.createObjectURL(new Blob([text], { type: `${mime};charset=utf-8` }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** A random signing secret in the Standard Webhooks form (`whsec_` and base64). */
export function newSecret(): string {
  const b = new Uint8Array(24);
  crypto.getRandomValues(b);
  return `whsec_${btoa(String.fromCharCode(...b))}`;
}

export const dayStart = () => new Date().toISOString().slice(0, 10);
