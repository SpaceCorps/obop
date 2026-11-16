/**
 * The identity of an app and the names it keeps on the phone, as one object that the app builds
 * once and hands to the host code. Nothing here knows which app it is: the values are the app's.
 *
 * This file imports nothing and uses only syntax that is erased as it stands (interfaces and type
 * annotations; no enums, namespaces or parameter properties), because `app.config.ts` reads the
 * app's identity too, and Expo evaluates that file in Node (see `config/ios.ts`, which follows the
 * same rule).
 *
 * The names are literals, never derived from the application id or from each other. A name that an
 * installed app has stored (the Keychain items, the database file) must come out the same in every
 * later build, or the stored sign-in and the unsent trips cannot be found.
 */

/** The places where the app keeps things on the phone. */
export interface StorageNames {
  /** The file of the encrypted database. */
  databaseFile: string;
  /** The Keychain or Keystore item that holds the key of the database. */
  databaseKey: string;
  /** The item that holds the server address and the client that the instance gave the app. */
  server: string;
  /** The item that holds the tokens of the sign-in. */
  tokens: string;
}

/** What an app says about itself. `scheme` and `redirectUri` are derived from the application id. */
export interface HostIdentity {
  /** Reverse domain form. Also the scheme of the app, the Android package and the base of the redirect. */
  readonly applicationId: string;
  /** The private-use scheme of the sign-in redirect: the application id. */
  readonly scheme: string;
  /** Where the system browser sends the person back to the app: `<scheme>:/oauth`. */
  readonly redirectUri: string;
  /** The name that the instance shows on its consent page. */
  readonly clientName: string;
  /** The address the sign-in screen starts with; empty for none. */
  readonly defaultServer: string;
  /** The app signs in to its default server only. */
  readonly serverLocked: boolean;
  /** The version of the app, `x.y.z`. */
  readonly version: string;
  /** The languages of the app, the first is the default. */
  readonly locales: readonly string[];
  readonly storage: Readonly<StorageNames>;
}

export interface IdentityInput {
  applicationId: string;
  clientName: string;
  defaultServer?: string;
  serverLocked?: boolean;
  version: string;
  locales: readonly string[];
  storage: StorageNames;
}

/** The path of the redirect, after the scheme. */
const REDIRECT_PATH = "/oauth";

/**
 * Why an application id cannot be used, or `null`. The id is the scheme of the redirect that the
 * instance checks (a private-use scheme: at least two labels of lower-case letters, digits and
 * hyphens), the Android package (segments of letters, digits and underscores, each starting with a
 * letter) and the iOS bundle id (letters, digits and hyphens). What all three accept is: at least
 * two segments, each starting with a lower-case letter, then lower-case letters and digits only.
 */
export function applicationIdProblem(id: string): string | null {
  if (id.length > 128) return "the application id may have at most 128 characters";
  const segments = id.split(".");
  if (segments.length < 2) {
    return "the application id needs at least two parts separated by dots, such as org.example.app";
  }
  for (const segment of segments) {
    if (segment === "")
      return "the application id has an empty part (two dots, or a dot at an end)";
    if (segment.length > 63) return "a part of the application id may have at most 63 characters";
    if (!/^[a-z][a-z0-9]*$/.test(segment)) {
      return `the part ${JSON.stringify(segment)} of the application id must start with a lower-case letter and hold only lower-case letters and digits (the Android package, the iOS bundle id and the redirect of the instance all have to accept it)`;
    }
  }
  return null;
}

/**
 * Why the instance would refuse `uri` as the redirect of a mobile app, or `null`: the rule of its
 * `private_use` check, written here a second time so that a build fails when it is made and not at
 * the first sign-in on a phone. A private-use scheme in reverse domain form with a path and nothing
 * else (RFC 8252 section 7.1).
 */
export function redirectProblem(uri: string): string | null {
  const colon = uri.indexOf(":");
  if (colon < 0) return "the redirect has no scheme";
  const scheme = uri.slice(0, colon);
  const path = uri.slice(colon + 1);
  if (scheme.length > 128 || uri.length > 256) return "the redirect is too long";
  if (!/^[a-z]/.test(scheme))
    return "the scheme of the redirect must start with a lower-case letter";
  const labels = scheme.split(".");
  if (labels.length < 2) return "the scheme of the redirect needs at least two parts";
  for (const label of labels) {
    if (label === "" || label.length > 63)
      return "a part of the scheme of the redirect has a bad length";
    if (label.startsWith("-") || label.endsWith("-")) {
      return "a part of the scheme of the redirect may not start or end with a hyphen";
    }
    if (!/^[a-z0-9-]+$/.test(label)) {
      return "the scheme of the redirect may hold lower-case letters, digits, hyphens and dots only";
    }
  }
  if (!path.startsWith("/") || path.startsWith("//")) {
    return "the redirect needs a path that starts with one slash and no authority";
  }
  if (!/^[A-Za-z0-9._~\-/]*$/.test(path))
    return "the path of the redirect holds a character that is not allowed";
  return null;
}

/**
 * Why a list of declared groups cannot be registered, or `null`: at most 32, each a group name of
 * letters, digits, dots, underscores and hyphens (at most 64 characters), none twice. The server
 * keeps its own list of groups that a phone may never hold; that list is not copied here.
 */
export function groupsProblem(groups: readonly string[]): string | null {
  if (groups.length > 32) return "at most 32 groups may be declared";
  const seen = new Set<string>();
  for (const group of groups) {
    if (group.length === 0 || group.length > 64 || !/^[A-Za-z0-9._-]+$/.test(group)) {
      return `${JSON.stringify(group)} is not a group name`;
    }
    if (seen.has(group)) return `${group} is declared twice`;
    seen.add(group);
  }
  return null;
}

const NAME = /^[A-Za-z0-9._-]+$/;

function hasControlCharacter(text: string): boolean {
  for (let i = 0; i < text.length; i += 1) {
    const code = text.charCodeAt(i);
    if (code < 32 || code === 127) return true;
  }
  return false;
}

function storageProblem(storage: StorageNames): string | null {
  const entries = Object.entries(storage) as [string, string][];
  for (const [key, value] of entries) {
    if (
      typeof value !== "string" ||
      value.length === 0 ||
      value.length > 128 ||
      !NAME.test(value)
    ) {
      return `storage.${key} must be 1 to 128 letters, digits, dots, underscores or hyphens`;
    }
    if (value.includes("..")) return `storage.${key} may not hold two dots in a row`;
  }
  if (new Set(entries.map(([, value]) => value)).size !== entries.length) {
    return "the four storage names must differ from each other";
  }
  return null;
}

/**
 * Builds the identity of an app and checks it. A value that cannot work throws here, when the app
 * starts or when the build is made, with a message that names the field.
 */
export function defineIdentity(input: IdentityInput): HostIdentity {
  const defaultServer = input.defaultServer ?? "";
  const serverLocked = input.serverLocked ?? false;
  const fail = (problem: string): never => {
    throw new Error(`identity: ${problem}`);
  };

  const idProblem = applicationIdProblem(input.applicationId);
  if (idProblem !== null) fail(idProblem);
  const redirectUri = `${input.applicationId}:${REDIRECT_PATH}`;
  const uriProblem = redirectProblem(redirectUri);
  if (uriProblem !== null) fail(uriProblem);

  const name = input.clientName;
  if (name.trim() !== name || name.length === 0 || name.length > 80 || hasControlCharacter(name)) {
    fail("clientName must be 1 to 80 characters, without control characters or spaces at the ends");
  }
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(input.version)) {
    fail(`version must be x.y.z, got ${JSON.stringify(input.version)}`);
  }
  if (input.locales.length === 0) fail("locales may not be empty");
  for (const locale of input.locales) {
    if (!/^[a-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$/.test(locale)) {
      fail(`${JSON.stringify(locale)} is not a language tag`);
    }
  }
  if (new Set(input.locales).size !== input.locales.length) fail("a locale is listed twice");
  if (defaultServer !== "" && !/^https?:\/\/\S+$/.test(defaultServer)) {
    fail("defaultServer must be empty or an http or https address");
  }
  if (serverLocked && defaultServer === "") fail("a locked server needs a defaultServer");
  const problem = storageProblem(input.storage);
  if (problem !== null) fail(problem);

  return Object.freeze({
    applicationId: input.applicationId,
    scheme: input.applicationId,
    redirectUri,
    clientName: name,
    defaultServer,
    serverLocked,
    version: input.version,
    locales: Object.freeze([...input.locales]),
    storage: Object.freeze({ ...input.storage }),
  });
}
