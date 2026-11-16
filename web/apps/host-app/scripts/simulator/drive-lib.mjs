// The command lines and the rules of the Simulator drive of the host (`drive.mjs` runs them). Pure,
// so that `vp test` checks them on any machine (scripts/simulator/drive.test.ts). Adapted from the
// Mileage app's scripts/simulator/ui/drive-lib.mjs: one test method that runs a script of actions.

/** The actions HostDrive.swift knows, by the number of words of their argument (-1: any text). */
export const ACTIONS = Object.freeze({
  launch: 0,
  activate: 0,
  terminate: 0,
  home: 0,
  swipeback: 0,
  swipeup: 0,
  swipedown: 0,
  notnow: 0,
  state: 0,
  keyboard: 0,
  sleep: 1,
  shot: 1,
  orient: 1,
  key: 1,
  clear: 1,
  dump: -1,
  wait: -1,
  waitsb: -1,
  waitgone: -1,
  exists: -1,
  tap: -1,
  tapsb: -1,
  tapif: -1,
  tapxy: 2,
  type: -1,
  frame: -1,
});

const SAFE_NAME = /^[A-Za-z0-9_.-]+$/;
const UDID = /^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$/;

/**
 * The problems of a script, one text per bad line (empty when it is fine). A line is an action and
 * its argument; `#` starts a comment.
 * @param {string} script
 * @returns {string[]}
 */
export function scriptProblems(script) {
  const problems = [];
  script.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) return;
    const [action, ...rest] = line.split(/\s+/);
    const words = Object.hasOwn(ACTIONS, action) ? ACTIONS[action] : undefined;
    if (words === undefined) {
      problems.push(`line ${i + 1}: unknown action ${JSON.stringify(action)}`);
    } else if (words >= 0 && rest.length !== words) {
      problems.push(`line ${i + 1}: ${action} takes ${words} word(s), got ${rest.length}`);
    } else if (words < 0 && rest.length === 0) {
      problems.push(`line ${i + 1}: ${action} needs an argument`);
    } else if (action === "shot" && !SAFE_NAME.test(rest[0])) {
      problems.push(`line ${i + 1}: a screenshot name has letters, digits and _ . - only`);
    } else if (action === "orient" && !["landscape", "portrait"].includes(rest[0])) {
      problems.push(`line ${i + 1}: orient takes landscape or portrait`);
    } else if (
      /^(wait|waitsb|waitgone)$/.test(action) &&
      !(Number(rest[0]) > 0 && rest.length > 1)
    ) {
      problems.push(`line ${i + 1}: ${action} takes seconds and a text`);
    }
  });
  return problems;
}

/**
 * `xcodebuild test` arguments for the UI-testing bundle that make-project.rb writes.
 * @param {{ project: string, device: string, derivedData: string, resultBundle: string }} options
 * @returns {string[]}
 */
export function testArgs({ project, device, derivedData, resultBundle }) {
  if (!UDID.test(device)) {
    throw new Error(`device must be the UDID of a Simulator, got ${JSON.stringify(device)}`);
  }
  return [
    "test",
    "-project",
    project,
    "-scheme",
    "HostDrive",
    "-destination",
    `id=${device}`,
    "-derivedDataPath",
    derivedData,
    "-resultBundlePath",
    resultBundle,
    "-only-testing:HostDrive/HostDrive/testScript",
    "CODE_SIGNING_ALLOWED=NO",
  ];
}

/** The names the Swift file reads from its environment. */
const RUNNER_VARIABLES = [
  "OBOP_APP_BUNDLE_ID",
  "OBOP_SCRIPT",
  "OBOP_DRIVER_USER",
  "OBOP_DRIVER_PASSWORD",
  "OBOP_SHOT_DIR",
];

/**
 * The environment for `xcodebuild`: the process environment plus the runner variables with the
 * prefix that `xcodebuild` strips (`TEST_RUNNER_`). A script that types $USER or $PASSWORD needs
 * the variable.
 * @param {Record<string, string | undefined>} env
 * @returns {Record<string, string | undefined>}
 */
export function runnerEnv(env) {
  const script = env.OBOP_SCRIPT ?? "";
  for (const [token, name] of [
    ["$USER", "OBOP_DRIVER_USER"],
    ["$PASSWORD", "OBOP_DRIVER_PASSWORD"],
  ]) {
    if (script.includes(token) && !env[name])
      throw new Error(`the script types ${token}: ${name} is not set`);
  }
  const out = { ...env };
  for (const name of RUNNER_VARIABLES) {
    if (env[name] !== undefined) out[`TEST_RUNNER_${name}`] = env[name];
  }
  return out;
}

/** The result of the session as `xcodebuild` prints it, or null for any other line. */
export function sessionVerdict(line) {
  const match = /^Test Suite 'Selected tests' (passed|failed) at /.exec(line);
  return match ? /** @type {"passed" | "failed"} */ (match[1]) : null;
}

/** Replaces every occurrence of each secret by `***`. */
export function redact(text, secrets) {
  let out = text;
  for (const secret of secrets) {
    if (secret.length >= 4) out = out.split(secret).join("***");
  }
  return out;
}
