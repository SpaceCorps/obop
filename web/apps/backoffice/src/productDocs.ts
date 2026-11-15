// Whether the pages that document the product (the Brand book and the Guide) are shown. An
// administrator can hide them for an instance that has its own brand (`hide_product_docs` of
// `instance_brand`, docs/spec/branding.md, section 6): they document obop, its mark and how to write
// a module for it, and a company that runs its own system does not want its staff pointed there.
//
// `productDocsVisible()` is the one predicate. Seven places of the app ask it and nothing else:
// the tab filter of Customize, `personalCustomize` in App, the account-menu item, the command
// palette entry, the notice and button of the Theme tab, the link on the Modules page and the link
// on the Build tab (held by `productDocs.test.ts`, `Chrome.docs.test.tsx` and
// `Customize.docs.test.tsx`). The value is read once after sign-in, with the metadata, so the
// screens do not show a link and then take it away; `App` puts it back to "shown" at sign-out, and a
// read that fails changes nothing (`App.productDocs.test.tsx`).

import { useSyncExternalStore } from "react";
import { call } from "./api.ts";

let hidden = false;
const listeners = new Set<() => void>();

/** Whether the Brand book and the Guide are shown. True until the switch is read as on. */
export function productDocsVisible(): boolean {
  return !hidden;
}

/** Sets the switch; the screens that subscribed render again when it changed. */
export function setProductDocsHidden(value: boolean): void {
  if (hidden === value) return;
  hidden = value;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** `productDocsVisible()` for a component that must render again when it changes. */
export function useProductDocsVisible(): boolean {
  return useSyncExternalStore(subscribe, productDocsVisible, productDocsVisible);
}

/**
 * The switch as the server has it: `instance_brand.get` answers it to every signed-in person. It is
 * on only when the answer is the boolean `true`; `undefined` when the answer could not be read, so
 * that a failed read changes nothing.
 */
export async function readProductDocsHidden(token: string): Promise<boolean | undefined> {
  const r = await call(token, "instance_brand.get");
  if (r.kind !== "ok") return undefined;
  const answer = r.value as { hide_product_docs?: unknown } | null;
  return answer !== null && typeof answer === "object"
    ? answer.hide_product_docs === true
    : undefined;
}

/** The tabs of Customize that document the product. */
export const PRODUCT_DOC_TABS: readonly string[] = ["brand", "guide"];

/** The tabs a person without the administrator right may open. */
const PERSONAL_TABS: readonly string[] = ["theme", "security", ...PRODUCT_DOC_TABS];

/**
 * The tabs of Customize one person sees: an administrator all of them, anyone else their own theme
 * and security (and the two documents), and nobody the two documents while they are hidden.
 */
export function visibleTabs<T extends { id: string }>(
  all: readonly T[],
  who: { isAdmin: boolean; docs: boolean },
): T[] {
  return all.filter(
    (x) =>
      (who.docs || !PRODUCT_DOC_TABS.includes(x.id)) &&
      (who.isAdmin || PERSONAL_TABS.includes(x.id)),
  );
}

/** Whether a tab id is one a person without the administrator right may open. */
export function isPersonalTab(id: string, docs: boolean): boolean {
  return PERSONAL_TABS.includes(id) && (docs || !PRODUCT_DOC_TABS.includes(id));
}
