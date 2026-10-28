import { en, type MessageKey, type Messages } from "./en.ts";

export type { MessageKey, Messages };

const catalogue: Messages = en;

/** A sentence of the catalogue with its `{placeholders}` filled. */
export function t(key: MessageKey, params: Record<string, string | number> = {}): string {
  return catalogue[key].replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}
