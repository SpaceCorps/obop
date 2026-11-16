// What `check.mjs` reads out of the texts adb gives it. Pure, so that `vp test` runs it on any
// machine (scripts/emulator/adb-lib.test.ts).

/** @typedef {{ left: number, top: number, right: number, bottom: number }} Bounds */
/** @typedef {{ id: string, text: string, description: string, className: string, enabled: boolean, bounds: Bounds }} UiNode */

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };

/** The text of an XML attribute value. */
function unescapeXml(text) {
  return text.replace(
    /&(?:#(\d+)|#x([0-9a-fA-F]+)|(amp|lt|gt|quot|apos));/g,
    (_, dec, hex, name) =>
      dec !== undefined
        ? String.fromCodePoint(Number(dec))
        : hex !== undefined
          ? String.fromCodePoint(Number.parseInt(hex, 16))
          : ENTITIES[name],
  );
}

/**
 * The nodes of a `uiautomator dump`, in document order. React Native puts a `testID` into
 * `resource-id` as it is; a view of Android has `package:id/name` there.
 * @param {string} xml
 * @returns {UiNode[]}
 */
export function parseUiDump(xml) {
  /** @type {UiNode[]} */
  const nodes = [];
  for (const tag of xml.matchAll(/<node\b([^>]*)>/g)) {
    /** @type {Record<string, string>} */
    const attributes = {};
    for (const [, name, value] of tag[1].matchAll(/([\w-]+)="([^"]*)"/g)) {
      attributes[name] = unescapeXml(value);
    }
    const bounds = /^\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]$/.exec(attributes.bounds ?? "");
    if (!bounds) continue;
    nodes.push({
      id: attributes["resource-id"] ?? "",
      text: attributes.text ?? "",
      description: attributes["content-desc"] ?? "",
      className: attributes.class ?? "",
      // A button that is disabled or busy says so here.
      enabled: attributes.enabled !== "false",
      bounds: {
        left: Number(bounds[1]),
        top: Number(bounds[2]),
        right: Number(bounds[3]),
        bottom: Number(bounds[4]),
      },
    });
  }
  return nodes;
}

/**
 * The first node with this `testID` (or Android id), or `undefined`.
 * @param {UiNode[]} nodes
 * @param {string} id
 * @returns {UiNode | undefined}
 */
export function nodeById(nodes, id) {
  return nodes.find((node) => node.id === id);
}

/**
 * Whether some node shows a text that holds `part`.
 * @param {UiNode[]} nodes
 * @param {string} part
 * @returns {boolean}
 */
export function hasText(nodes, part) {
  return nodes.some((node) => node.text.includes(part));
}

/**
 * The point to tap for a node: the middle of its bounds.
 * @param {Bounds} bounds
 * @returns {{ x: number, y: number }}
 */
export function centre(bounds) {
  return {
    x: Math.round((bounds.left + bounds.right) / 2),
    y: Math.round((bounds.top + bounds.bottom) / 2),
  };
}

/**
 * The DevTools sockets of WebViews in `/proc/net/unix`: `webview_devtools_remote_<pid>`. A build
 * without OBOP_WEBVIEW_DEBUG=1 opens none.
 * @param {string} procNetUnix
 * @returns {{ name: string, pid: number }[]}
 */
export function devtoolsSockets(procNetUnix) {
  const found = new Map();
  for (const [, name, pid] of procNetUnix.matchAll(/@(webview_devtools_remote_(\d+))\b/g)) {
    found.set(name, { name, pid: Number(pid) });
  }
  return [...found.values()];
}

/**
 * The package of the activity on top, from `dumpsys activity activities`. `undefined` when the
 * text names none.
 * @param {string} dumpsys
 * @returns {string | undefined}
 */
export function resumedPackage(dumpsys) {
  // Android 10 and later print `topResumedActivity=`, earlier ones and some lines `mResumedActivity:`.
  for (const name of ["topResumedActivity", "mResumedActivity", "ResumedActivity"]) {
    const match = new RegExp(`\\b${name}[=:]\\s*ActivityRecord\\{\\S+ \\S+ ([\\w.]+)/`).exec(
      dumpsys,
    );
    if (match) return match[1];
  }
  return undefined;
}

/**
 * The package of the home screen, from
 * `cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME`.
 * @param {string} output
 * @returns {string | undefined}
 */
export function homePackage(output) {
  const match = /^([\w.]+)\/\S+\s*$/m.exec(output);
  return match ? match[1] : undefined;
}

/**
 * The argument of `adb shell input text` for an address, quoted for the device's shell. Only the
 * characters an address of this check has are taken, so nothing in it can be read as shell text.
 * @param {string} text
 * @returns {string}
 */
export function inputTextArgument(text) {
  if (!/^[A-Za-z0-9:/._[\]-]+$/.test(text)) {
    throw new Error(`refusing to type ${JSON.stringify(text)}: only the characters of an address`);
  }
  return `'${text}'`;
}

/**
 * The user and the password of the throwaway account: a file of `user=` and `password=` lines.
 * @param {string} text
 * @returns {{ user: string, password: string } | undefined}
 */
export function parseAccount(text) {
  /** @type {Record<string, string>} */
  const values = {};
  for (const line of text.split(/\r?\n/)) {
    const match = /^\s*(user|password)=(.*)$/.exec(line);
    if (match) values[match[1]] = match[2];
  }
  return values.user && values.password
    ? { user: values.user, password: values.password }
    : undefined;
}

/**
 * The page of the instance among the targets of `/json/list`: a page whose address is on the
 * origin. `undefined` when there is none (yet).
 * @param {unknown} targets
 * @param {string} origin
 * @returns {{ url: string, webSocketDebuggerUrl: string } | undefined}
 */
export function instancePage(targets, origin) {
  if (!Array.isArray(targets)) return undefined;
  return targets.find(
    (target) =>
      typeof target === "object" &&
      target !== null &&
      target.type === "page" &&
      typeof target.webSocketDebuggerUrl === "string" &&
      typeof target.url === "string" &&
      (target.url === origin || target.url.startsWith(`${origin}/`)),
  );
}
