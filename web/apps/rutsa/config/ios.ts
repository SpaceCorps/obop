// The pure parts of the iOS configuration (`app.config.ts` applies them), kept here so that
// `vp test` can run them without Expo.

/** Reverse domain form: letters, digits, hyphens and dots, at least two parts (Apple's rule for a bundle id). */
const BUNDLE_ID = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

/**
 * The bundle id of the iOS build: `OBOP_IOS_BUNDLE_ID` when set, else `fallback` (the application
 * id). A test build signed with a free Apple ID takes another id (for example
 * `org.obop.rutsa.test`): an App ID first signed in a free personal team can later not be
 * registered by the SpaceCorps team (Q37 of docs/DECISIONS.md), as found with the Mileage app. The
 * value goes into the Xcode project, so it must have the right form.
 */
export function iosBundleId(env: string | undefined, fallback: string): string {
  if (env === undefined || env === "") return fallback;
  if (!BUNDLE_ID.test(env)) {
    throw new Error(
      `OBOP_IOS_BUNDLE_ID must be a bundle id in reverse domain form (letters, digits, hyphens and dots), got ${JSON.stringify(env)}`,
    );
  }
  return env;
}

/**
 * Takes out of Info.plist the entries that `expo-dev-client` adds for a development build: the
 * Bonjour service `_expo._tcp` (finding a dev server on the network) and the URL scheme `exp+<slug>`
 * (opening a project from the launcher). Any other build has no use for them. The dev launcher's
 * own build phase removes only the Bonjour service, and in an incremental Xcode build it can run
 * too early (docs/spec/mobile.md 8.8, item 6), so they are taken out of the generated project.
 */
export function withoutDevClientEntries<T extends object>(plist: T): T {
  const result = { ...plist } as Record<string, unknown>;
  if (Array.isArray(result.NSBonjourServices)) {
    const left = result.NSBonjourServices.filter((name) => name !== "_expo._tcp");
    if (left.length > 0) result.NSBonjourServices = left;
    else delete result.NSBonjourServices;
  }
  if (Array.isArray(result.CFBundleURLTypes)) {
    const isDevClientScheme = (type: unknown): boolean => {
      const schemes =
        typeof type === "object" && type !== null
          ? (type as { CFBundleURLSchemes?: unknown }).CFBundleURLSchemes
          : undefined;
      return (
        Array.isArray(schemes) &&
        schemes.length > 0 &&
        schemes.every((s) => typeof s === "string" && s.startsWith("exp+"))
      );
    };
    const left = result.CFBundleURLTypes.filter((type) => !isDevClientScheme(type));
    if (left.length > 0) result.CFBundleURLTypes = left;
    else delete result.CFBundleURLTypes;
  }
  return result as T;
}
