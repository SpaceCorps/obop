// Copied from web/apps/mileage-app/config/ios.ts: `iosBundleId` and `withoutDevClientEntries`,
// unchanged (the two are merged once both apps are on main). The usage texts are the host's own.
//
// The pure parts of the iOS configuration (`app.config.ts` applies them), kept here so that
// `vp test` can run them without Expo.

/** Reverse domain form: letters, digits, hyphens and dots, at least two parts (Apple's rule for a bundle id). */
const BUNDLE_ID = /^[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

/**
 * The bundle id of the iOS build: `OBOP_IOS_BUNDLE_ID` when set, else `fallback`. It is kept apart
 * from the scheme (which is the application id) because an App ID first signed in a free personal
 * team can later not be registered by the SpaceCorps team (Q37 of docs/DECISIONS.md: the final id
 * is the owner's). The value goes into the Xcode project, so it must have the right form.
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
 * The entries that `expo-dev-client` adds to Info.plist for a development build: the Bonjour
 * service `_expo._tcp` (finding a dev server on the network) and the URL scheme `exp+<slug>`
 * (opening a project from the launcher). A build that is not a development build has no use for
 * either. The dev launcher has a build phase of its own that is meant to remove the Bonjour service
 * from a Release build (`[Expo Dev Launcher] Strip Local Network Keys for Release`, expo-dev-launcher
 * 58.0.14): it deletes only `_expo._tcp` and never the URL scheme, and in an incremental Xcode build
 * it can run before the Info.plist is processed, so a build that started from a generated plist
 * with the entries can ship them (docs/spec/mobile.md 8.8, item 6). Hence they are taken out of the
 * generated project and not left to that phase.
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
 * The four texts iOS shows when it asks the person for something, by their Info.plist key. iOS
 * ends an app that reaches for one of these without its text. With media capture denied in the
 * WebView, the camera and microphone texts serve the file picker of the system only (a photo or a
 * video taken for an upload); the page itself cannot make iOS ask.
 */
export const IOS_USAGE_TEXTS = Object.freeze({
  NSLocalNetworkUsageDescription:
    "obop connects to the obop server whose address you enter. If that server is on your local network, iOS asks you to allow this.",
  NSCameraUsageDescription:
    "obop uses the camera when you choose to take a photo for a record, for example a receipt.",
  NSMicrophoneUsageDescription:
    "obop uses the microphone when you choose to record a video for a record.",
  NSPhotoLibraryAddUsageDescription:
    "obop saves a picture to your photos when you choose to save it.",
});
