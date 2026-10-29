// The command lines of the iOS build helper (`xcode.mjs` runs them). Pure, so that `vp test` checks
// them on any machine (scripts/ios/xcode.test.ts).

const SAFE_NAME = /^[A-Za-z0-9_.-]+$/;
const CONFIGURATIONS = new Set(["Debug", "Release"]);
const SAFE_ARCHS = /^[a-z0-9_]+( [a-z0-9_]+)*$/;

/**
 * `xcodebuild` arguments for a build for the iOS Simulator. No signing: the Simulator needs none,
 * and nothing here signs in to an Apple account.
 *
 * @param {{ workspace: string, scheme: string, configuration?: string, archs?: string, derivedData: string }} options
 * @returns {string[]}
 */
export function simulatorBuildArgs(options) {
  const { workspace, scheme, derivedData } = options;
  const configuration = options.configuration ?? "Release";
  const archs = options.archs ?? "arm64";
  if (!CONFIGURATIONS.has(configuration)) {
    throw new Error(`configuration must be Debug or Release, got ${JSON.stringify(configuration)}`);
  }
  if (!SAFE_ARCHS.test(archs)) {
    throw new Error(
      `architectures must be names like arm64 or "arm64 x86_64", got ${JSON.stringify(archs)}`,
    );
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
    "CODE_SIGNING_ALLOWED=NO",
    "CODE_SIGNING_REQUIRED=NO",
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
