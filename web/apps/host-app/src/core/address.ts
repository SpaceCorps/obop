// The address of the instance as a person types it and as the app keeps it.
//
// The form is `https://host[:port]` and optionally one path segment (`https://shop.example.com/admin`),
// because the back office of an instance with a public website is under a prefix. The address is
// read by hand and its origin comes from `originOf` of @obop/host-bridge: React Native's `URL` is
// not the URL standard's (it keeps a user name, capitals and a default port in `origin`), and the
// app compares origins as text everywhere.
import { originOf } from "@obop/host-bridge";
import { isLocalHost } from "./local-host.ts";

/** An address the app may use: the origin, and the path with a slash at each end (`/` when none). */
export interface Address {
  readonly origin: string;
  readonly path: string;
}

export type AddressProblem =
  /** Nothing was typed. */
  | "empty"
  /** A scheme other than http and https. */
  | "scheme"
  /** A user name, a password, a query, a fragment, a space or a backslash. */
  | "extras"
  /** A letter outside ASCII in the host: the person types the `xn--` form. */
  | "non_ascii"
  /** The host or the port is not one. */
  | "host"
  /** More than one path segment, or one the server would not take as a prefix. */
  | "path"
  /** Plain http in a build that speaks https only. */
  | "cleartext"
  /** Plain http, in a build that allows it, to a host that is not this device or the local network. */
  | "cleartext_public";

export type AddressResult = ({ ok: true } & Address) | { ok: false; problem: AddressProblem };

/** What the build allows. `cleartext`: the build was made with OBOP_ALLOW_CLEARTEXT=1. */
export interface AddressOptions {
  readonly cleartext: boolean;
}

/**
 * One path segment as the server takes an admin prefix (obop-http `admin_prefix_ok`): lower-case
 * letters, digits, `-` and `_`, at most 32, with or without a slash at the end. Or no path.
 */
const PATH = /^\/(?:([a-z0-9_-]{1,32})\/?)?$/;
// Every character from the space to the tilde: what an address may be written in.
const PRINTABLE_ASCII = /^[\x20-\x7e]*$/;

/** The host of an origin that `originOf` wrote, without the port: `[::1]`, `10.0.2.2`, `shop.example.com`. */
export function hostOf(origin: string): string {
  const authority = origin.slice(origin.indexOf("://") + 3);
  if (authority.startsWith("[")) return authority.slice(0, authority.indexOf("]") + 1);
  const colon = authority.indexOf(":");
  return colon < 0 ? authority : authority.slice(0, colon);
}

/** The host and the port of an origin as the host bar shows them: `shop.example.com`, `localhost:18180`. */
export function hostAndPort(origin: string): string {
  return origin.slice(origin.indexOf("://") + 3);
}

/** The origin speaks plain http. */
export function isCleartext(origin: string): boolean {
  return origin.startsWith("http://");
}

/** The address as a person reads and types it: `https://shop.example.com/admin`, no slash at the end. */
export function addressText(address: Address): string {
  return address.path === "/" ? address.origin : `${address.origin}${address.path.slice(0, -1)}`;
}

/** The address the WebView is started on: the origin and the path, `https://shop.example.com/admin/`. */
export function startAddress(address: Address): string {
  return `${address.origin}${address.path}`;
}

/**
 * Reads an address. A text without `://` is read as an https address, so `shop.example.com/admin`
 * is `https://shop.example.com/admin`; plain http has to be written out.
 *
 * The scheme rule is part of the reading, and it runs on every address before the app uses it: a
 * typed one, one a server forwards to, and the stored one at every start. `https` always passes;
 * `http` passes only in a build that allows it and only to this device or the local network.
 */
export function parseAddress(input: string, options: AddressOptions): AddressResult {
  const refuse = (problem: AddressProblem): AddressResult => ({ ok: false, problem });
  const text = input.trim();
  if (text === "") return refuse("empty");

  const withScheme = /^([A-Za-z][A-Za-z0-9+.-]*):\/\//.exec(text);
  // `mailto:someone` and `https:/host` name a scheme and are not addresses of this form;
  // `localhost:18180` is a host and a port.
  if (withScheme === null && /^[A-Za-z][A-Za-z0-9+.-]*:(?![0-9])/.test(text)) {
    return refuse("scheme");
  }
  const scheme = withScheme === null ? "https" : (withScheme[1] ?? "").toLowerCase();
  if (scheme !== "https" && scheme !== "http") return refuse("scheme");
  const rest = withScheme === null ? text : text.slice(withScheme[0].length);

  const slash = rest.indexOf("/");
  const authority = slash < 0 ? rest : rest.slice(0, slash);
  const path = slash < 0 ? "" : rest.slice(slash);
  if (/[\s\\?#@]/.test(rest)) return refuse("extras");
  // A look-alike letter cannot be typed: the host is ASCII, and a name with other letters is
  // typed in its `xn--` form.
  if (!PRINTABLE_ASCII.test(authority)) return refuse("non_ascii");

  const origin = originOf(`${scheme}://${authority}`);
  if (origin === null) return refuse("host");

  const segment = PATH.exec(path === "" ? "/" : path);
  if (segment === null) return refuse("path");

  if (scheme === "http") {
    if (!options.cleartext) return refuse("cleartext");
    if (!isLocalHost(hostOf(origin))) return refuse("cleartext_public");
  }
  return { ok: true, origin, path: segment[1] === undefined ? "/" : `/${segment[1]}/` };
}
