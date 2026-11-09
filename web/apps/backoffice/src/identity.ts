// The Identity tab of Customize: the instance's name, logo, messages and titles, read with
// `instance_brand.get` and written with `instance_brand.configure` and `instance_brand.clear`, and
// nothing else (docs/spec/branding.md, sections 3 and 6). This file is the part with no screen in it.
import { LANGS, type PageBrand } from "@obop/ui";
import { call } from "./api.ts";

export type Lang = (typeof LANGS)[number];

/** What the form holds. Every text is as typed; an empty one is "not set". */
export type Form = {
  name: string;
  slug: string;
  titles: Record<Lang, string>;
  messages: Record<Lang, string>;
  hideDocs: boolean;
};

/** What the person did to the artwork. */
export type LogoChange =
  | { kind: "keep" }
  | { kind: "replace"; text: string; file: string }
  | { kind: "remove" };

/** The most the artwork may be, in bytes (the strict reader of the server refuses more). */
export const LOGO_MAX_BYTES = 16 * 1024;
/** The limits of the texts, in characters; the server counts them too and is the judge. */
export const LIMITS = { name: 40, slug: 30, title: 60, message: 240 } as const;

/**
 * `text` cut after `max` characters, counted as the server counts them: Unicode code points, not
 * the UTF-16 units that `String.length` and the `maxLength` of an input count (a letter outside the
 * Basic Multilingual Plane is two units and one character, and a pair is never cut in half).
 */
export function clip(text: string, max: number): string {
  if (text.length <= max) return text;
  const characters = Array.from(text);
  return characters.length <= max ? text : characters.slice(0, max).join("");
}

/** The record as the page works with it. */
export type Loaded = {
  form: Form;
  /** Whether a row exists. With none, every surface shows the product's own name. */
  set: boolean;
  /** The version of the logo; the logo route carries it so that a new logo is a new address. */
  v: string;
  /** The effective slug and the kind of logo that is served: `none`, `tile` or `artwork`. */
  slug: string;
  logo: string;
  /** The fields whose stored value cannot be used, and the word for why. */
  fallbacks: { field: string; reason: string }[];
};

const empty = (): Record<Lang, string> =>
  Object.fromEntries(LANGS.map((l) => [l, ""])) as Record<Lang, string>;

export const blankForm = (): Form => ({
  name: "",
  slug: "",
  titles: empty(),
  messages: empty(),
  hideDocs: false,
});

const text = (x: unknown): string => (typeof x === "string" ? x : "");

/** A JSON text or the value itself, as the REST surface may give an object-valued field. */
function json(x: unknown): unknown {
  if (typeof x !== "string") return x;
  try {
    return JSON.parse(x);
  } catch {
    return null;
  }
}

/** The answer of `instance_brand.get`, or `null` when it is not what the server writes. */
export function readBrand(answer: unknown): Loaded | null {
  if (answer === null || typeof answer !== "object") return null;
  const a = answer as Record<string, unknown>;
  const form = blankForm();
  form.name = text(a.name);
  form.slug = text(a.slug);
  for (const l of LANGS) {
    form.titles[l] = text(a[`title_${l}`]);
    form.messages[l] = text(a[`message_${l}`]);
  }
  form.hideDocs = a.hide_product_docs === true;
  const effective = (json(a.effective) ?? {}) as Record<string, { value?: unknown }>;
  const fallbacks = json(a.fallbacks);
  return {
    form,
    set: a.set === true,
    v: text(a.v),
    slug: text(effective.slug?.value),
    logo: text(effective.logo?.value) || "none",
    fallbacks: Array.isArray(fallbacks)
      ? fallbacks
          .filter((f): f is { field: string; reason: string } => typeof f?.field === "string")
          .map((f) => ({ field: f.field, reason: text(f.reason) }))
      : [],
  };
}

/** `instance_brand.get`; `null` when it could not be read. */
export async function loadBrand(token: string): Promise<Loaded | null> {
  const r = await call(token, "instance_brand.get");
  return r.kind === "ok" ? readBrand(r.value) : null;
}

/**
 * The input of `configure` for what changed. Only the fields that differ are sent: a field that is
 * sent is checked by the whole policy even when it equals the stored one, and a stored value that an
 * older policy let in would then be refused for nothing. An empty text clears a field, the artwork
 * is cleared by `""`, and the switch is sent as a boolean. The switch alone cannot be typed over: a
 * stored value that is not a boolean reads as off, and the person has nothing to change, so it is
 * sent whenever the record lists it among the `unusable` fields, and saving puts a boolean there.
 */
export function changes(
  base: Form,
  now: Form,
  logo: LogoChange,
  unusable: readonly string[] = [],
): Record<string, string | boolean> {
  const out: Record<string, string | boolean> = {};
  if (now.name !== base.name) out.name = now.name;
  if (now.slug !== base.slug) out.slug = now.slug;
  for (const l of LANGS) {
    if (now.titles[l] !== base.titles[l]) out[`title_${l}`] = now.titles[l];
    if (now.messages[l] !== base.messages[l]) out[`message_${l}`] = now.messages[l];
  }
  if (logo.kind === "replace") out.logo_svg = logo.text;
  else if (logo.kind === "remove") out.logo_svg = "";
  if (now.hideDocs !== base.hideDocs || unusable.includes("hide_product_docs"))
    out.hide_product_docs = now.hideDocs;
  return out;
}

/** The brand the sign-in card preview draws from the form, with the address of the logo. */
export function previewBrand(form: Form, v: string): PageBrand {
  // `PageBrand["messages"]` is read-only, so the object is built as a writable one first.
  const messages: Partial<Record<Lang, string>> = {};
  for (const l of LANGS) if (form.messages[l].trim()) messages[l] = form.messages[l];
  return {
    name: form.name.trim() || "…",
    slug: form.slug || "preview",
    messages,
    v,
  };
}

/** Whether a file the person chose can be the artwork: an SVG of at most 16 KiB. */
export function artworkProblem(file: {
  name: string;
  size: number;
  type?: string;
}): "big" | "kind" | null {
  if (file.size > LOGO_MAX_BYTES) return "big";
  const svg = file.type === "image/svg+xml" || /\.svg$/i.test(file.name);
  return svg ? null : "kind";
}
