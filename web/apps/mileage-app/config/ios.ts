// The pure parts of the iOS configuration (`app.config.ts` applies them), kept here so that
// `vp test` can run them without Expo.

/** Reverse domain form: letters, digits, hyphens and dots, at least two parts (Apple's rule for a bundle id). */
const BUNDLE_ID = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

/**
 * The bundle id of the iOS build: `OBOP_IOS_BUNDLE_ID` when set, else `fallback`. It is kept apart
 * from the sign-in scheme (which is the application id) because an App ID first signed in a free
 * personal team can later not be registered by the SpaceCorps team (Q37 of docs/DECISIONS.md: the
 * final id is the owner's). The value goes into the Xcode project, so it must have the right form.
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
 * The background modes the app declares. `expo-task-manager` adds `fetch` and the app uses no
 * background fetch (an unused mode is a store review question); `location` is the only one it
 * needs. Anything else that a library adds stays, and is listed by the release gate.
 */
export function withoutFetchMode(modes: unknown): string[] {
  if (!Array.isArray(modes)) return [];
  return modes.filter((mode): mode is string => typeof mode === "string" && mode !== "fetch");
}

/**
 * The entries that `expo-dev-client` adds to Info.plist for a development build: the Bonjour
 * service `_expo._tcp` (finding a dev server on the network) and the URL scheme `exp+<slug>`
 * (opening a project from the launcher). A build that is not a development build has no use for
 * either. The strip step of the dev launcher that is meant to remove them from a Release build did
 * not run before the plist was processed in an incremental build (docs/spec/mobile.md 8.8), so
 * they are taken out of the generated project and not left to that step.
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

/**
 * `OBOP_RECORD_THINNING`: `off` stores every fix the phone delivers (a TEST build only: the app
 * honours it only where plain http is allowed too, and the release gate fails a release build that
 * carries it). Unset or empty keeps the thinning. Anything else is a typo and stops the build.
 */
export function recordThinningOff(env: string | undefined): boolean {
  if (env === undefined || env === "") return false;
  if (env === "off") return true;
  throw new Error(`OBOP_RECORD_THINNING must be "off" or unset, got ${JSON.stringify(env)}`);
}
