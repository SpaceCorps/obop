// The command lines and the line rules of the Simulator drive (`drive.mjs` runs them). Pure, so that
// `vp test` checks them on any machine (scripts/simulator/ui/drive.test.ts).

/** The test methods of MileageDrive.swift for the whole trip, in the order they must run in one session. */
export const TRIP_STEPS = [
  "test01_SignIn",
  "test02_Start",
  "test03_Background",
  "test04_Foreground",
  "test05_Stop",
  "test06_Submit",
];

/** A line of the test output that the host reacts to: the trip is recording, so the route may start. */
export const ROUTE_MARKER = "RECORDING-SHOWN";

const SAFE_NAME = /^[A-Za-z0-9_.-]+$/;
const UDID = /^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$/;
const TEST_METHOD = /^test[0-9A-Za-z_]+$/;

/** The names the Swift file reads from its environment (docs/spec/mobile.md 8.8). */
const RUNNER_VARIABLES = [
  "OBOP_APP_BUNDLE_ID",
  "OBOP_DRIVER_USER",
  "OBOP_DRIVER_PASSWORD",
  "OBOP_SHOT_DIR",
  "OBOP_SERVER_ADDRESS",
];

/**
 * `xcodebuild test` arguments for the UI-testing bundle that make-project.rb writes. One session
 * runs every named method: the Simulator clears its simulated route when a test session ends or
 * another starts, so a route that is fed from outside has to span one session.
 *
 * @param {{ project: string, scheme?: string, device: string, derivedData: string, resultBundle: string, only: string[] }} options
 * @returns {string[]}
 */
export function testArgs(options) {
  const { project, device, derivedData, resultBundle, only } = options;
  const scheme = options.scheme ?? "MileageDrive";
  if (!UDID.test(device)) {
    throw new Error(`device must be the UDID of a Simulator, got ${JSON.stringify(device)}`);
  }
  if (!SAFE_NAME.test(scheme)) {
    throw new Error(
      `the scheme name ${JSON.stringify(scheme)} has characters other than letters, digits and _ . -`,
    );
  }
  if (only.length === 0) throw new Error("at least one test method is needed");
  for (const method of only) {
    if (!TEST_METHOD.test(method)) {
      throw new Error(`not a test method name: ${JSON.stringify(method)}`);
    }
  }
  return [
    "test",
    "-project",
    project,
    "-scheme",
    scheme,
    "-destination",
    `id=${device}`,
    "-derivedDataPath",
    derivedData,
    "-resultBundlePath",
    resultBundle,
    ...only.map((method) => `-only-testing:MileageDrive/MileageDrive/${method}`),
    "CODE_SIGNING_ALLOWED=NO",
  ];
}

/**
 * The environment for `xcodebuild`: the process environment plus the runner variables with the
 * prefix that `xcodebuild` strips (`TEST_RUNNER_`). Only the names the Swift file reads are passed
 * on, and the user and the password must be there.
 *
 * @param {Record<string, string | undefined>} env
 * @returns {Record<string, string | undefined>}
 */
export function runnerEnv(env) {
  for (const required of ["OBOP_DRIVER_USER", "OBOP_DRIVER_PASSWORD"]) {
    if (!env[required]) throw new Error(`${required} is not set`);
  }
  const out = { ...env };
  for (const name of RUNNER_VARIABLES) {
    if (env[name] !== undefined) out[`TEST_RUNNER_${name}`] = env[name];
  }
  return out;
}

/** True for the line that says the trip is recording (the whole line, so a log of the source does not match). */
export function isRouteMarker(line) {
  return line.trim() === ROUTE_MARKER;
}

/**
 * The result of the session as `xcodebuild` prints it ("passed" or "failed"), or null for any other
 * line. After this line `xcodebuild` may hang for minutes, so the caller stops it.
 *
 * @param {string} line
 * @returns {"passed" | "failed" | null}
 */
export function sessionVerdict(line) {
  const match = /^Test Suite 'Selected tests' (passed|failed) at /.exec(line);
  return match ? /** @type {"passed" | "failed"} */ (match[1]) : null;
}

/**
 * Replaces every occurrence of each secret by `***`, so that a log or a console never holds one.
 *
 * @param {string} text
 * @param {string[]} secrets
 */
export function redact(text, secrets) {
  let out = text;
  for (const secret of secrets) {
    if (secret.length >= 4) out = out.split(secret).join("***");
  }
  return out;
}
