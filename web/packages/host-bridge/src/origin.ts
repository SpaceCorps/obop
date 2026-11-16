// The origin of an http or https address, worked out by hand.
//
// Why not `new URL(address).origin`: the three places this package runs in do not agree on it. A
// browser and Node follow the URL standard. React Native does not: its `URL` (react-native 0.88.0,
// Libraries/Blob/URL.js) is a set of regular expressions, and its `origin` is the text up to the
// first slash as it was written, so it keeps a user name, capitals and a default port, and gives
// an empty text for anything that is not http or https. The host decides by an exact comparison
// of origins whom it answers, so the same text has to come out everywhere.
//
// What this gives is the origin of the URL standard, or `null`. It never gives another origin than
// the standard does: where the standard would rewrite an address in a way that is not done here
// (a user name, a host written as one number, an IPv6 address with a dotted part), the answer is
// `null`, which every caller treats as "not the instance". One limit: a label that begins with
// `xn--` is not decoded, so an address that the standard refuses for a broken label of that kind
// still gets an origin here. No document can be loaded from such an address.

const DEFAULT_PORT: Readonly<Record<string, string>> = { http: "80", https: "443" };
/** The longest host and port this looks at (a DNS name has at most 253 characters). */
const MAX_AUTHORITY_CHARS = 300;

/** `a.b.c.d` with each part 0 to 255 and no leading zero: the only spelling the standard leaves as it is. */
function isDottedQuad(host: string): boolean {
  const parts = host.split(".");
  return (
    parts.length === 4 &&
    parts.every((part) => /^(0|[1-9][0-9]{0,2})$/.test(part) && Number(part) <= 255)
  );
}

/** The standard's spelling of an IPv6 address (the text between the brackets, lower case), or `null`. */
function canonicalIpv6(text: string): string | null {
  if (!/^[0-9a-f:]+$/.test(text)) return null;
  const halves = text.split("::");
  if (halves.length > 2) return null;
  const groupsOf = (part: string): number[] | null => {
    if (part === "") return [];
    const groups: number[] = [];
    for (const group of part.split(":")) {
      if (!/^[0-9a-f]{1,4}$/.test(group)) return null;
      groups.push(Number.parseInt(group, 16));
    }
    return groups;
  };
  const head = groupsOf(halves[0] ?? "");
  if (head === null) return null;
  let groups: number[];
  if (halves.length === 2) {
    const tail = groupsOf(halves[1] ?? "");
    if (tail === null) return null;
    const missing = 8 - head.length - tail.length;
    if (missing < 1) return null;
    groups = [...head, ...Array.from({ length: missing }, () => 0), ...tail];
  } else {
    if (head.length !== 8) return null;
    groups = head;
  }
  // The first of the longest runs of zero groups, when it is two groups or more, becomes `::`.
  let bestStart = -1;
  let bestLength = 0;
  for (let i = 0; i < 8;) {
    if (groups[i] !== 0) {
      i += 1;
      continue;
    }
    let end = i;
    while (end < 8 && groups[end] === 0) end += 1;
    if (end - i > bestLength) {
      bestStart = i;
      bestLength = end - i;
    }
    i = end;
  }
  const hex = (from: number, to: number) =>
    groups
      .slice(from, to)
      .map((group) => group.toString(16))
      .join(":");
  if (bestLength < 2) return hex(0, 8);
  return `${hex(0, bestStart)}::${hex(bestStart + bestLength, 8)}`;
}

/** A host name of ASCII letters, digits and hyphens in labels separated by single dots, or `null`. */
function canonicalName(host: string): string | null {
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)*$/.test(host)) return null;
  const last = host.slice(host.lastIndexOf(".") + 1);
  // The standard reads a host whose last label is a number (decimal, or hex after `0x`) as an
  // IPv4 address and rewrites it: `127.1` is `127.0.0.1`, `0x7f.1` too, `010.0.0.1` is `8.0.0.1`.
  if (/^([0-9]+|0x[0-9a-f]*)$/.test(last)) return isDottedQuad(host) ? host : null;
  return host;
}

/**
 * The origin of `address`, `scheme://host` or `scheme://host:port`, or `null`.
 *
 * `null` for: anything that is not text; a scheme other than http and https (`about:blank`,
 * `data:`, `file:` and `blob:` among them); an address with a user name or a password; a host
 * that is not ASCII letters, digits, hyphens and dots, a dotted IPv4 address or a bracketed IPv6
 * address; a port that is not a number up to 65535. The scheme and the host come out in lower
 * case and the default port (80, 443) is left out, as the URL standard writes an origin.
 */
export function originOf(address: string | null | undefined): string | null {
  if (typeof address !== "string") return null;
  const start = /^(https?):\/\//i.exec(address);
  if (start === null) return null;
  const scheme = (start[1] ?? "").toLowerCase();
  const rest = address.slice(start[0].length);
  const end = rest.search(/[/?#]/);
  const authority = (end < 0 ? rest : rest.slice(0, end)).toLowerCase();
  if (authority === "" || authority.length > MAX_AUTHORITY_CHARS) return null;

  let host: string;
  let port: string;
  if (authority.startsWith("[")) {
    const close = authority.indexOf("]");
    if (close < 0) return null;
    const inside = canonicalIpv6(authority.slice(1, close));
    if (inside === null) return null;
    host = `[${inside}]`;
    port = authority.slice(close + 1);
  } else {
    const colon = authority.indexOf(":");
    const name = canonicalName(colon < 0 ? authority : authority.slice(0, colon));
    if (name === null) return null;
    host = name;
    port = colon < 0 ? "" : authority.slice(colon);
  }

  if (port === "" || port === ":") return `${scheme}://${host}`;
  if (!/^:[0-9]{1,10}$/.test(port)) return null;
  const number = Number(port.slice(1));
  if (number > 65_535) return null;
  const text = String(number);
  return text === DEFAULT_PORT[scheme] ? `${scheme}://${host}` : `${scheme}://${host}:${text}`;
}
