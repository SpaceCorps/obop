// The scripted checks of the host app on an Android emulator: the must-pass group of the design
// (E1, E16a, E2a, E3, E6, E7, E12). One line per check; the first failure ends the run with status 1.
//
//   vp run emulator:check                       # the defaults below
//   node scripts/emulator/check.mjs [--serial emulator-5580] [--server http://localhost:18180]
//                                   [--account FILE] [--shots DIR]
//
// What it needs:
//   - an emulator (ANDROID_SERIAL or --serial; default emulator-5580) with a TEST build of the app
//     installed (OBOP_ALLOW_CLEARTEXT=1 and OBOP_WEBVIEW_DEBUG=1: the page is driven over the
//     WebView's DevTools socket, which only such a build opens);
//   - a THROWAWAY obop instance on this computer at --server (the emulator reaches it through
//     `adb reverse`, as `localhost`, which is a secure context);
//   - its account: --account FILE or OBOP_HOST_ACCOUNT=FILE (lines `user=` and `password=`), or
//     OBOP_HOST_USER and OBOP_HOST_PASSWORD.
//
// What it does to the emulator: it clears the app's data (E1 needs the first launch), types into
// the address screen, signs in, and for E12 takes the `adb reverse` of the server's port away and
// puts it back, which is how the server is "stopped" for the app without touching the instance.
//
// Native screens are read with `uiautomator dump` (React Native puts a testID into resource-id) and
// driven with `input tap` and `input text`. Everything in the page goes over DevTools:
// Runtime.evaluate, Input.insertText and Input.dispatchKeyEvent. The script never prints or stores
// the password or the session token: of the token it reports only whether it is there.
//
// Exit status: 0 all passed, 1 a check failed, 2 the run could not start.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import {
  centre,
  devtoolsSockets,
  hasText,
  homePackage,
  inputTextArgument,
  instancePage,
  nodeById,
  parseAccount,
  parseUiDump,
  resumedPackage,
} from "./adb-lib.mjs";

/** The application id of every build of this slice (config/identity.ts). */
const PACKAGE = "org.obop.app.test";
const ACTIVITY = `${PACKAGE}/.MainActivity`;
/** The local port the DevTools socket of the WebView is forwarded to. */
const DEVTOOLS_PORT = 9333;
/** The script's own responder for E1: something that answers, and is no obop server. */
const RESPONDER_PORT = 18181;
/** Another origin, for E7. The site need not answer. */
const FOREIGN = "https://example.com/";
const DUMP_FILE = "/sdcard/obop-host-ui.xml";
/** How long one DevTools command may take. */
const COMMAND_TIMEOUT_MS = 15_000;

function stop(message) {
  console.error(`check.mjs: ${message}`);
  process.exit(2);
}

// ---- options ----------------------------------------------------------------------------------

const args = process.argv.slice(2);
const OPTIONS = new Set(["--serial", "--server", "--account", "--shots"]);
for (let i = 0; i < args.length; i += 2) {
  if (!OPTIONS.has(args[i])) stop(`unknown option ${args[i]}`);
  if (args[i + 1] === undefined || args[i + 1].startsWith("--")) stop(`${args[i]} takes a value`);
}
const option = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};

const serial = option("--serial") ?? process.env.ANDROID_SERIAL ?? "emulator-5580";
const server = (option("--server") ?? "http://localhost:18180").replace(/\/+$/, "");
const serverMatch = /^http:\/\/localhost:(\d{2,5})$/.exec(server);
if (!serverMatch) {
  stop(
    `--server must be http://localhost:PORT (a secure context through adb reverse), got ${server}`,
  );
}
const serverPort = Number(serverMatch[1]);
const serverHost = `localhost:${serverPort}`;
const shots = option("--shots");

const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
if (!sdk) stop("ANDROID_HOME is not set");
const adbPath = resolve(sdk, "platform-tools", process.platform === "win32" ? "adb.exe" : "adb");
if (!existsSync(adbPath)) stop(`${adbPath} does not exist`);

function readAccount() {
  const file = option("--account") ?? process.env.OBOP_HOST_ACCOUNT;
  if (file !== undefined) {
    if (!existsSync(file)) stop(`the account file ${file} does not exist`);
    const account = parseAccount(readFileSync(file, "utf8"));
    if (!account) stop(`the account file ${file} needs a user= and a password= line`);
    return account;
  }
  const user = process.env.OBOP_HOST_USER;
  const password = process.env.OBOP_HOST_PASSWORD;
  if (!user || !password) {
    stop(
      "no account: give --account FILE, OBOP_HOST_ACCOUNT, or OBOP_HOST_USER and OBOP_HOST_PASSWORD",
    );
  }
  return { user, password };
}
const account = readAccount();

// ---- adb --------------------------------------------------------------------------------------

/** One adb command on the emulator; its standard output. */
function adb(words, { mayFail = false, binary = false } = {}) {
  const result = spawnSync(adbPath, ["-s", serial, ...words], {
    encoding: binary ? "buffer" : "utf8",
    maxBuffer: 64 * 1024 * 1024,
    // No adb command of this script takes long; one that hangs must not hang the run.
    timeout: 60_000,
  });
  if (result.error) throw new Error(`adb ${words.join(" ")}: ${result.error.message}`);
  if (result.status !== 0 && !mayFail) {
    throw new Error(`adb ${words.join(" ")} failed: ${String(result.stderr).trim()}`);
  }
  return result.stdout;
}
const shell = (command, options) => adb(["shell", command], options);

/** The nodes on the screen. The dump fails while something on the screen moves, so it is tried again. */
async function screen() {
  for (let attempt = 0; attempt < 4; attempt += 1) {
    const said = shell(`uiautomator dump ${DUMP_FILE}`, { mayFail: true });
    if (said.includes("dumped to")) {
      return parseUiDump(adb(["exec-out", "cat", DUMP_FILE]));
    }
    await sleep(500);
  }
  return [];
}

/** Waits until the screen satisfies `test`; returns its nodes. */
async function untilScreen(test, what, timeout = 30_000) {
  const end = Date.now() + timeout;
  for (;;) {
    const nodes = await screen();
    if (test(nodes)) return nodes;
    if (Date.now() > end) throw new Error(`the screen never showed ${what}`);
    await sleep(500);
  }
}

function tap(node) {
  const { x, y } = centre(node.bounds);
  shell(`input tap ${x} ${y}`);
}

const resumed = () => resumedPackage(shell("dumpsys activity activities"));

async function untilResumed(test, what, timeout = 20_000) {
  const end = Date.now() + timeout;
  for (;;) {
    const name = resumed();
    if (test(name)) return name;
    if (Date.now() > end)
      throw new Error(`${what}; the activity on top is of ${name ?? "nothing"}`);
    await sleep(500);
  }
}

/**
 * Starts the app, or brings it back on top, with the intent of a tap on its icon. It does not wait
 * (`am start -W` never returns when the task is only brought to the front); the callers wait for
 * what they need to see.
 *
 * One difference to a tap on the icon remains: Android keeps a task at Back on its root only when
 * the home screen started it. A start by adb is not one, so the third Back of E6 ends the activity
 * and the page's session with it.
 */
function startApp() {
  shell(
    `am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -n ${ACTIVITY}`,
  );
}

/**
 * Ends the app's process and starts it from the home screen, so that the home screen is what lies
 * under it (and not whatever app an earlier check or run left on top).
 */
async function startFromHome() {
  shell(`am force-stop ${PACKAGE}`);
  shell("input keyevent 3");
  await untilResumed((name) => name !== PACKAGE, "the home screen did not come on top");
  startApp();
}

function saveShot(name) {
  if (shots === undefined) return undefined;
  try {
    mkdirSync(shots, { recursive: true });
    const file = resolve(shots, name);
    writeFileSync(file, adb(["exec-out", "screencap", "-p"], { binary: true }));
    return file;
  } catch {
    return undefined;
  }
}

// ---- the page, over DevTools ------------------------------------------------------------------

/** A DevTools connection to the page of the instance in the app's WebView. */
class Page {
  #socket;
  #next = 1;
  #waiting = new Map();

  static async open() {
    const end = Date.now() + 30_000;
    let last = "the app has no process";
    for (;;) {
      const pid = Number(shell(`pidof ${PACKAGE}`, { mayFail: true }).trim().split(/\s+/)[0]);
      if (Number.isInteger(pid) && pid > 0) {
        const socket = devtoolsSockets(shell("cat /proc/net/unix")).find((s) => s.pid === pid);
        if (!socket) {
          last = "its WebView has no DevTools socket (is this a build with OBOP_WEBVIEW_DEBUG=1?)";
        } else {
          adb(["forward", `tcp:${DEVTOOLS_PORT}`, `localabstract:${socket.name}`]);
          try {
            const answer = await fetch(`http://127.0.0.1:${DEVTOOLS_PORT}/json/list`);
            const target = instancePage(await answer.json(), server);
            if (target) return await Page.#connect(target.webSocketDebuggerUrl);
            last = `no page of ${server} in the WebView`;
          } catch (error) {
            last = `DevTools did not answer: ${error instanceof Error ? error.message : String(error)}`;
          }
        }
      }
      if (Date.now() > end) throw new Error(`could not reach the page: ${last}`);
      await sleep(700);
    }
  }

  static #connect(url) {
    return new Promise((done, failed) => {
      const page = new Page();
      const socket = new WebSocket(url);
      page.#socket = socket;
      socket.addEventListener("open", () => done(page));
      socket.addEventListener("error", () => failed(new Error("the DevTools socket failed")));
      socket.addEventListener("close", () => {
        for (const waiter of page.#waiting.values()) waiter.failed(new Error("DevTools closed"));
        page.#waiting.clear();
      });
      socket.addEventListener("message", (event) => {
        const message = JSON.parse(String(event.data));
        const waiter = page.#waiting.get(message.id);
        if (!waiter) return;
        page.#waiting.delete(message.id);
        // A DevTools error is passed on without the text of the command: it may hold the password.
        if (message.error) waiter.failed(new Error(`DevTools refused ${waiter.method}`));
        else waiter.done(message.result);
      });
    });
  }

  /**
   * One DevTools command. The page answers only while the app is on top (Android stops the
   * renderer of a WebView in the background), so a command that gets no answer fails here.
   */
  send(method, params = {}) {
    return new Promise((done, failed) => {
      const id = this.#next++;
      const timer = setTimeout(() => {
        this.#waiting.delete(id);
        failed(new Error(`DevTools did not answer ${method} in ${COMMAND_TIMEOUT_MS / 1000} s`));
      }, COMMAND_TIMEOUT_MS);
      const settle = (finish) => (value) => {
        clearTimeout(timer);
        finish(value);
      };
      this.#waiting.set(id, { done: settle(done), failed: settle(failed), method });
      this.#socket.send(JSON.stringify({ id, method, params }));
    });
  }

  /** The value of an expression in the page. */
  async evaluate(expression, { userGesture = false } = {}) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
      userGesture,
    });
    if (result.exceptionDetails) {
      throw new Error(`the page threw: ${result.exceptionDetails.exception?.description ?? "?"}`);
    }
    return result.result.value;
  }

  /** Waits until an expression is true in the page. */
  async until(expression, what, timeout = 30_000) {
    const end = Date.now() + timeout;
    for (;;) {
      if ((await this.evaluate(expression)) === true) return;
      if (Date.now() > end) throw new Error(`the page never had ${what}`);
      await sleep(300);
    }
  }

  /** Types into the focused element, as a keyboard would. */
  insert(text) {
    return this.send("Input.insertText", { text });
  }

  async enter() {
    const key = {
      key: "Enter",
      code: "Enter",
      windowsVirtualKeyCode: 13,
      nativeVirtualKeyCode: 13,
    };
    await this.send("Input.dispatchKeyEvent", { type: "keyDown", text: "\r", ...key });
    await this.send("Input.dispatchKeyEvent", { type: "keyUp", ...key });
  }

  close() {
    this.#socket?.close();
  }
}

const HAS_SIGN_IN = `document.querySelector('input[name="username"]') !== null`;
const SIGNED_IN = `document.querySelector("header.topbar") !== null`;

// ---- the address screen -----------------------------------------------------------------------

/** Replaces the text of the address field and taps Connect. */
async function connectTo(address) {
  const before = await untilScreen(
    (nodes) => nodeById(nodes, "address-field"),
    "the address field",
  );
  const field = nodeById(before, "address-field");
  tap(field);
  await sleep(600);
  // To the end of the text, then one Delete per character that is there.
  shell("input keyevent 123");
  const deletes = Array.from({ length: field.text.length + 4 }, () => "67").join(" ");
  shell(`input keyevent ${deletes}`);
  shell(`input text ${inputTextArgument(address)}`);
  const typed = await untilScreen(
    (nodes) =>
      nodeById(nodes, "address-field")?.text === address &&
      nodeById(nodes, "address-connect")?.enabled === true,
    `${address} in the address field`,
  );
  const connect = nodeById(typed, "address-connect");
  if (!connect) throw new Error("the address screen has no Connect button");
  tap(connect);
}

// ---- the checks -------------------------------------------------------------------------------

/** @type {Page | undefined} */
let page;
let responder;

async function e1() {
  responder = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end("<!doctype html><title>Not obop</title><p>Something else answers here.</p>");
  });
  await new Promise((done, failed) => {
    responder.once("error", failed);
    responder.listen(RESPONDER_PORT, "127.0.0.1", done);
  });
  adb(["reverse", `tcp:${serverPort}`, `tcp:${serverPort}`]);
  adb(["reverse", `tcp:${RESPONDER_PORT}`, `tcp:${RESPONDER_PORT}`]);

  shell(`am force-stop ${PACKAGE}`);
  shell(`pm clear ${PACKAGE}`);
  await startFromHome();
  await untilScreen(
    (nodes) => nodeById(nodes, "address-field") && nodeById(nodes, "address-connect"),
    "the address screen",
  );

  await connectTo(`http://localhost:${RESPONDER_PORT}`);
  const refused = await untilScreen(
    (nodes) => hasText(nodes, "not as an obop server"),
    'the "not an obop server" sentence',
  );
  if (!nodeById(refused, "address-field") || nodeById(refused, "web-screen")) {
    throw new Error("the app left the address screen for an address that is no obop server");
  }

  await connectTo(server);
  await untilScreen(
    (nodes) => nodeById(nodes, "web-screen") && !nodeById(nodes, "address-field"),
    `the page of ${server}`,
  );
  return `the address screen is shown at first launch; http://localhost:${RESPONDER_PORT} (200, text/html) is refused as no obop server; ${server} is accepted`;
}

async function e16a() {
  await startFromHome();
  const nodes = await untilScreen(
    (found) =>
      nodeById(found, "host-bar-host")?.text === serverHost && hasText(found, "test build"),
    `the host bar with ${serverHost} and "test build"`,
  );
  if (nodeById(nodes, "address-field")) throw new Error("the app asked for the address again");
  return `after a start the host bar names ${serverHost} and says "test build" (the address was stored)`;
}

async function e2a() {
  page = await Page.open();
  const agent = await page.evaluate("navigator.userAgent");
  if (typeof agent !== "string" || !/\bObopHost\/1\b/.test(agent)) {
    throw new Error(`the user agent does not hold ObopHost/1: ${agent}`);
  }
  if ((await page.evaluate("isSecureContext")) !== true) {
    throw new Error("the page is not a secure context");
  }
  return `the user agent ends in "${agent.slice(agent.indexOf("ObopHost/"))}"; isSecureContext is true`;
}

/** Signs in on the sign-in page: focus, typed text, Enter. */
async function signIn() {
  await page.until(HAS_SIGN_IN, "the sign-in form");
  await page.evaluate(`document.querySelector('input[name="username"]').focus()`);
  await page.insert(account.user);
  await page.evaluate(`document.querySelector('input[name="password"]').focus()`);
  await page.insert(account.password);
  await page.enter();
  await page.until(SIGNED_IN, "the top bar of the signed-in back office (header.topbar)");
}

async function e3() {
  await signIn();
  // Only whether it is there: the value never leaves the page.
  const present = await page.evaluate(
    `(() => { const t = sessionStorage.getItem("obop-token"); return typeof t === "string" && t.length > 0; })()`,
  );
  if (present !== true) throw new Error("signed in, but the page holds no session token");
  return `signed in as ${account.user}: header.topbar exists and the session token is present`;
}

/**
 * Clicks the first link of the page that `pick` takes, as a person clicks; returns its address or
 * null. `pick` is page code: a function of the link's href and the link.
 */
function clickLink(pick) {
  return page.evaluate(
    `(() => { const a = [...document.querySelectorAll('a[href^="#/"]')].find((x) => (${pick})(x.getAttribute("href"), x)); if (!a) return null; a.click(); return a.getAttribute("href"); })()`,
    { userGesture: true },
  );
}

async function e6() {
  const start = await page.evaluate("location.hash");
  const tiles = await page.evaluate(
    `[...document.querySelectorAll('a.app-tile[href^="#/"]')].map((a) => a.getAttribute("href"))`,
  );
  if (!Array.isArray(tiles) || tiles.length === 0) throw new Error("the launcher has no app tile");
  // An app from the launcher, then a list or a record of it: the first link in the app that leads
  // somewhere else. An app with one place only (an empty list) is left again through the history
  // of the page, and the next tile is tried.
  let app;
  let list;
  for (const tile of tiles) {
    const opened = await clickLink(
      `(h, a) => a.classList.contains("app-tile") && h === ${JSON.stringify(tile)}`,
    );
    if (opened !== tile) throw new Error(`the launcher lost its tile ${tile}`);
    await page.until(`location.hash === ${JSON.stringify(tile)}`, `the app ${tile}`);
    await page.until(SIGNED_IN, "the top bar in the app");
    const end = Date.now() + 4_000;
    while (typeof list !== "string" && Date.now() < end) {
      list = await clickLink(`(h) => h !== "#/" && h !== location.hash`);
      if (typeof list !== "string") await sleep(400);
    }
    if (typeof list === "string") {
      app = tile;
      break;
    }
    await page.evaluate("history.back()");
    await page.until(`location.hash === ${JSON.stringify(start)}`, "the launcher again");
    await page.until(`document.querySelector("a.app-tile") !== null`, "the tiles of the launcher");
  }
  if (typeof app !== "string" || typeof list !== "string") {
    throw new Error("no app of the launcher has a second place to open");
  }
  await page.until(`location.hash === ${JSON.stringify(list)}`, `the place ${list}`);

  shell("input keyevent 4");
  await page.until(`location.hash === ${JSON.stringify(app)}`, `${app} again after Back`);
  shell("input keyevent 4");
  await page.until(`location.hash === ${JSON.stringify(start)}`, "the launcher again after Back");
  if ((await page.evaluate(SIGNED_IN)) !== true) throw new Error("Back signed the person out");

  const home = homePackage(
    shell(
      "cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME",
    ),
  );
  if (!home) throw new Error("could not ask Android for its home screen");
  shell("input keyevent 4");
  await untilResumed(
    (name) => name === home,
    "the third Back did not leave the app for the launcher",
  );
  return `opened ${app}, then ${shorten(list)}; Back twice walked location.hash back; the third Back left the app (on top: ${home})`;
}

const shorten = (text) => (text.length > 48 ? `${text.slice(0, 45)}...` : text);

/**
 * Brings the app back on top after another app took the screen, and waits until it stays there: a
 * browser that is still starting (its first-run screen comes a moment after its first activity)
 * can take the screen once more.
 */
async function returnToApp() {
  const end = Date.now() + 30_000;
  let steady = 0;
  while (steady < 3) {
    if (resumed() === PACKAGE) {
      steady += 1;
    } else {
      steady = 0;
      if (Date.now() > end) {
        throw new Error(`the app did not come back on top; on top is ${resumed() ?? "nothing"}`);
      }
      startApp();
    }
    await sleep(500);
  }
}

async function e7() {
  // The third Back of E6 ended the activity: adb did not start it "from the home screen", which is
  // Android's condition for keeping a task at Back. The app starts again with a new page, and the
  // session of the old one (sessionStorage) is gone with it.
  page?.close();
  await returnToApp();
  page = await Page.open();
  await page.until(`(${HAS_SIGN_IN}) || (${SIGNED_IN})`, "the page after the start");
  if ((await page.evaluate(SIGNED_IN)) !== true) await signIn();
  const address = await page.evaluate("location.href");

  // (a) the document itself is sent to another origin.
  await page.evaluate(`void (location.href = ${JSON.stringify(FOREIGN)})`);
  const browserA = await untilResumed(
    (name) => name !== undefined && name !== PACKAGE,
    `(a) no other app took the screen for ${FOREIGN}`,
  );
  // The page is asked once the app is on top again: in the background its renderer stands still.
  await returnToApp();
  if ((await page.evaluate("location.href")) !== address) {
    throw new Error("(a) the WebView left the instance");
  }

  // (b) a link that asks for a new window, clicked as a person clicks.
  await page.evaluate(
    `(() => { const a = document.createElement("a"); a.target = "_blank"; a.href = ${JSON.stringify(FOREIGN)}; a.textContent = "elsewhere"; document.body.append(a); a.click(); a.remove(); })()`,
    { userGesture: true },
  );
  const browserB = await untilResumed(
    (name) => name !== undefined && name !== PACKAGE,
    `(b) no other app took the screen for a new window of ${FOREIGN}`,
  );
  await returnToApp();
  if ((await page.evaluate("location.href")) !== address) {
    throw new Error("(b) the WebView left the instance");
  }
  if ((await page.evaluate(SIGNED_IN)) !== true) throw new Error("(b) the person was signed out");
  return `(a) location.href = "${FOREIGN}" and (b) a click on a target="_blank" link each left the WebView on ${address} and put another app on top (${browserA}, ${browserB})`;
}

async function e12() {
  page?.close();
  page = undefined;
  // The server is gone for the app: the emulator's localhost port leads nowhere.
  adb(["reverse", "--remove", `tcp:${serverPort}`]);
  try {
    await startFromHome();
    const failed = await untilScreen(
      (nodes) => nodeById(nodes, "error-screen") && nodeById(nodes, "error-retry"),
      "the error screen with Try again",
    );
    if (!hasText(failed, serverHost)) throw new Error("the error screen does not name the server");
    // Still gone: Try again keeps the error screen.
    tap(nodeById(failed, "error-retry"));
    await sleep(1_500);
    // The button is busy while the try runs, and enabled again when it has failed.
    const still = await untilScreen(
      (nodes) =>
        nodeById(nodes, "error-screen") && nodeById(nodes, "error-retry")?.enabled === true,
      "the error screen again after Try again",
    );
    adb(["reverse", `tcp:${serverPort}`, `tcp:${serverPort}`]);
    tap(nodeById(still, "error-retry"));
  } finally {
    adb(["reverse", `tcp:${serverPort}`, `tcp:${serverPort}`]);
  }
  await untilScreen(
    (nodes) => nodeById(nodes, "web-screen") && !nodeById(nodes, "error-screen"),
    "the page instead of the error screen",
  );
  page = await Page.open();
  await page.until(HAS_SIGN_IN, "the sign-in page after Try again");
  return `with the server unreachable a start shows the error screen naming ${serverHost}, and Try again keeps it; with the server back, Try again shows the sign-in page`;
}

/** @type {[string, () => Promise<string>][]} */
const CHECKS = [
  ["E1", e1],
  ["E16a", e16a],
  ["E2a", e2a],
  ["E3", e3],
  ["E6", e6],
  ["E7", e7],
  ["E12", e12],
];

// ---- the run ----------------------------------------------------------------------------------

function cleanUp() {
  page?.close();
  responder?.close();
  adb(["forward", "--remove", `tcp:${DEVTOOLS_PORT}`], { mayFail: true });
  adb(["reverse", "--remove", `tcp:${RESPONDER_PORT}`], { mayFail: true });
}

async function preflight() {
  if (adb(["get-state"], { mayFail: true }).trim() !== "device") {
    stop(`no device ${serial}: start the emulator first`);
  }
  if (!shell(`pm path ${PACKAGE}`, { mayFail: true }).includes("package:")) {
    stop(`${PACKAGE} is not installed on ${serial}: install the test APK first`);
  }
  try {
    const answer = await fetch(`http://127.0.0.1:${serverPort}/api/openapi.json`);
    if (!answer.ok) stop(`the instance at 127.0.0.1:${serverPort} answered ${answer.status}`);
  } catch {
    stop(`no instance answers at 127.0.0.1:${serverPort}: start the throwaway instance first`);
  }
}

await preflight();
let failed = false;
for (const [name, check] of CHECKS) {
  try {
    console.log(`ok    ${name.padEnd(5)} ${await check()}`);
  } catch (error) {
    failed = true;
    console.log(
      `FAIL  ${name.padEnd(5)} ${error instanceof Error ? error.message : String(error)}`,
    );
    const shot = saveShot(`check-failed-${name}.png`);
    if (shot) console.log(`      the screen at the failure: ${shot}`);
    break;
  }
}
cleanUp();
console.log(failed ? "emulator check FAILED" : `emulator check passed (${CHECKS.length} checks)`);
process.exit(failed ? 1 : 0);
