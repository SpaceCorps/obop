// Which hosts a test build may reach over plain http: this device and the local network, and
// nothing else. The host is given as it stands in an origin that `originOf` of @obop/host-bridge
// wrote: lower case, an IPv4 address as four plain numbers, an IPv6 address in brackets and in the
// spelling of the URL standard.

/** The first group of an IPv6 address in the standard's spelling (`fe80::1` gives 0xfe80, `::1` gives 0). */
function firstGroup(address: string): number {
  const text = address.split(":")[0] ?? "";
  return text === "" ? 0 : Number.parseInt(text, 16);
}

function isLocalV6(address: string): boolean {
  // The loopback address.
  if (address === "::1") return true;
  const first = firstGroup(address);
  // fe80::/10, the link-local addresses, and fc00::/7, the unique local ones.
  return (first >= 0xfe80 && first <= 0xfebf) || (first >= 0xfc00 && first <= 0xfdff);
}

function isLocalV4(a: number, b: number): boolean {
  return (
    a === 127 || // 127/8, this device
    a === 10 || // 10/8
    (a === 172 && b >= 16 && b <= 31) || // 172.16/12
    (a === 192 && b === 168) || // 192.168/16
    (a === 169 && b === 254) // 169.254/16, link-local
  );
}

/**
 * `localhost`, a name under `.localhost` or `.local`, an IPv4 address in 127/8, 10/8, 172.16/12,
 * 192.168/16 or 169.254/16, or an IPv6 address that is `::1` or in `fe80::/10` or `fc00::/7`.
 * An IPv4 address written inside an IPv6 one (`::ffff:7f00:1`) is not local here.
 */
export function isLocalHost(hostname: string): boolean {
  if (hostname === "localhost") return true;
  if (hostname.endsWith(".localhost") || hostname.endsWith(".local")) return true;
  if (hostname.startsWith("[") && hostname.endsWith("]")) {
    return isLocalV6(hostname.slice(1, -1));
  }
  const quad = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(hostname);
  if (quad === null) return false;
  return isLocalV4(Number(quad[1]), Number(quad[2]));
}
