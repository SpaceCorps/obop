import { en, type MessageKey } from "./en.ts";

export type { MessageKey };

type Params = Record<string, string | number>;

/**
 * A sentence of the catalogue with its `{placeholders}` filled. A placeholder without a value
 * stays as it is written, so a missing one shows in the text and in the tests.
 */
export function t(key: MessageKey, params: Params = {}): string {
  return en[key].replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}
