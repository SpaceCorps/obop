export type AddressResult =
  | { ok: true; address: string }
  | { ok: false; problem: "address" | "cleartext" };

/**
 * The address of the instance as the app uses it: `https` (or `http`, only in a build made for a
 * test instance), host and port, nothing else. It is built from the parsed address, so a harmless
 * spelling (capitals, a default port, a slash at the end) gives the same text as the one the
 * instance reports as its issuer. A user name, a password, a path, a query or a fragment is refused.
 */
export function checkAddress(input: string, cleartextAllowed: boolean): AddressResult {
  let url: URL;
  try {
    url = new URL(input.trim());
  } catch {
    return { ok: false, problem: "address" };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    return { ok: false, problem: "address" };
  }
  if (url.username !== "" || url.password !== "" || url.search !== "" || url.hash !== "") {
    return { ok: false, problem: "address" };
  }
  if (url.pathname !== "/" || url.host === "") return { ok: false, problem: "address" };
  if (url.protocol === "http:" && !cleartextAllowed) return { ok: false, problem: "cleartext" };
  return { ok: true, address: `${url.protocol}//${url.host}` };
}
