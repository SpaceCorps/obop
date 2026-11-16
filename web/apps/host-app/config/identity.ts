// The identity of the host app, in one place. `app.config.ts` reads it for the name, the scheme,
// the Android package, the iOS bundle id and the version; the app reads the same object for the
// name it stores the server under.
//
// Node reads this file when the app is built, so it imports nothing and uses only syntax that is
// erased as it stands (no enums, namespaces or parameter properties).

/**
 * Why an application id cannot be used, or `null`. Copied unchanged from
 * web/apps/mileage-app/src/core/host-config.ts (`applicationIdProblem`; the two are merged once
 * both apps are on main). The id is the scheme of the app, the Android package (segments of
 * letters, digits and underscores, each starting with a letter) and the iOS bundle id (letters,
 * digits and hyphens). What all of them accept is: at least two segments, each starting with a
 * lower-case letter, then lower-case letters and digits only.
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

/** What the host app says about itself. */
export interface HostAppIdentity {
  /** Reverse domain form. Also the scheme of the app and the Android package. */
  readonly applicationId: string;
  /** The scheme of the app: the application id. The app ignores every address it is opened with. */
  readonly scheme: string;
  /** The name under the icon. */
  readonly name: string;
  /** The name of the project at Expo; the development client builds its scheme (`exp+<slug>`) from it. */
  readonly slug: string;
  /** The version of the app, `x.y.z`. It is also in the user agent of the WebView. */
  readonly version: string;
  /** The version code of Android: a whole number that only grows. */
  readonly versionCode: number;
  /** The names the app keeps things under on the phone. */
  readonly storage: {
    /** The secure-store item that holds the address of the instance. */
    readonly server: string;
  };
}

function defineIdentity(input: Omit<HostAppIdentity, "scheme">): HostAppIdentity {
  const fail = (problem: string): never => {
    throw new Error(`identity: ${problem}`);
  };
  const idProblem = applicationIdProblem(input.applicationId);
  if (idProblem !== null) fail(idProblem);
  if (input.name.trim() !== input.name || input.name.length === 0 || input.name.length > 30) {
    fail("name must be 1 to 30 characters without spaces at the ends");
  }
  if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(input.slug)) {
    fail("slug must be lower-case letters, digits and single hyphens");
  }
  if (!/^\d+\.\d+\.\d+$/.test(input.version)) {
    fail(`version must be x.y.z, got ${JSON.stringify(input.version)}`);
  }
  if (!Number.isInteger(input.versionCode) || input.versionCode < 1) {
    fail("versionCode must be a whole number of 1 or more");
  }
  const server = input.storage.server;
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(server) || server.includes("..")) {
    fail("storage.server must be 1 to 128 letters, digits, dots, underscores or hyphens");
  }
  return Object.freeze({
    applicationId: input.applicationId,
    scheme: input.applicationId,
    name: input.name,
    slug: input.slug,
    version: input.version,
    versionCode: input.versionCode,
    storage: Object.freeze({ server }),
  });
}

/**
 * PLACEHOLDER: "org.obop.app.test" is a stand-in application id for every build of this slice. It
 * is not registered anywhere and no store account, signing key or publication exists (Q37 of
 * docs/DECISIONS.md). Once an app is on a store the id is hard to change, so the owner rules first.
 *
 * The storage name is what an installed app already has in its Keychain or Keystore. It is written
 * out and is not derived from the application id: changing it loses the stored server.
 * `config/identity.test.ts` pins every value.
 */
export const identity: HostAppIdentity = defineIdentity({
  applicationId: "org.obop.app.test",
  name: "obop",
  slug: "obop-host",
  version: "0.1.0",
  versionCode: 1,
  storage: { server: "obop.host.server" },
});
