// The command lines of the iOS build helper (`xcode.mjs` runs them). Pure, so that `vp test` checks
// them on any machine (scripts/ios/xcode.test.ts). Adapted from the Mileage app's
// scripts/ios/xcode-lib.mjs: the host builds for the Simulator only, so there is no device line.

const SAFE_NAME = /^[A-Za-z0-9_.-]+$/;
const CONFIGURATIONS = new Set(["Debug", "Release"]);
const SAFE_ARCHS = /^[a-z0-9_]+( [a-z0-9_]+)*$/;
const SIGNING = new Set(["adhoc", "none"]);

/**
 * `xcodebuild` arguments for a build for the iOS Simulator. Nothing here names an account, a team,
 * a certificate or a provisioning profile, and nothing signs in to Apple.
 *
 * - `adhoc` (the default): signed with the identity "-", which needs no certificate. An unsigned app
 *   carries no entitlements, and the Keychain of the Simulator then refuses every call of
 *   expo-secure-store ("A required entitlement is not present"; seen for the Mileage app,
 *   docs/spec/mobile.md 8.8, and for the host, docs/spec/mobile-host.md 12.8): the app could not
 *   store the address of its server.
 * - `none`: `CODE_SIGNING_ALLOWED=NO`, no signature at all.
 *
 * @param {{ workspace: string, scheme: string, configuration?: string, archs?: string, derivedData: string, signing?: string }} options
 * @returns {string[]}
 */
export function simulatorBuildArgs(options) {
  const { workspace, scheme, derivedData } = options;
  const configuration = options.configuration ?? "Release";
  const archs = options.archs ?? "arm64";
  const signing = options.signing ?? "adhoc";
  if (!CONFIGURATIONS.has(configuration)) {
    throw new Error(`configuration must be Debug or Release, got ${JSON.stringify(configuration)}`);
  }
  if (!SAFE_ARCHS.test(archs)) {
    throw new Error(
      `architectures must be names like arm64 or "arm64 x86_64", got ${JSON.stringify(archs)}`,
    );
  }
  if (!SIGNING.has(signing)) {
    throw new Error(`signing must be adhoc or none, got ${JSON.stringify(signing)}`);
  }
  for (const [what, value] of [
    ["scheme", scheme],
    ["workspace", workspace.replace(/^.*[\\/]/, "")],
  ]) {
    if (!SAFE_NAME.test(value)) {
      throw new Error(
        `the ${what} name ${JSON.stringify(value)} has characters other than letters, digits and _ . -`,
      );
    }
  }
  const sign =
    signing === "none"
      ? ["CODE_SIGNING_ALLOWED=NO", "CODE_SIGNING_REQUIRED=NO"]
      : ["CODE_SIGN_IDENTITY=-", "CODE_SIGNING_REQUIRED=NO", "CODE_SIGNING_ALLOWED=YES"];
  return [
    "-workspace",
    workspace,
    "-scheme",
    scheme,
    "-configuration",
    configuration,
    "-sdk",
    "iphonesimulator",
    "-destination",
    "generic/platform=iOS Simulator",
    "-derivedDataPath",
    derivedData,
    ...sign,
    // No team and no profile, whatever the project says.
    "DEVELOPMENT_TEAM=",
    "PROVISIONING_PROFILE_SPECIFIER=",
    `ARCHS=${archs}`,
    "ONLY_ACTIVE_ARCH=NO",
    "build",
  ];
}

/** Where `xcodebuild` puts the app of such a build. */
export function simulatorAppPath(options) {
  const configuration = options.configuration ?? "Release";
  return `${options.derivedData}/Build/Products/${configuration}-iphonesimulator/${options.scheme}.app`;
}
