// The pure part of check-trip.mjs (E2): what it reads in a dump of the screen (`adb shell
// uiautomator dump`) and how it judges the km. Pure, so that `vp test` checks it on any machine
// (scripts/emulator/check-lib.test.ts).

/**
 * @typedef {{ id: string, text: string, desc: string, bounds: [number, number, number, number] }} UiNode
 */

/** An attribute of a `<node>` of the dump, with the entities of XML undone. */
function attribute(tag, name) {
  const match = new RegExp(`\\s${name}="([^"]*)"`).exec(tag);
  if (!match) return "";
  return match[1]
    .replaceAll("&quot;", '"')
    .replaceAll("&apos;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

/**
 * The nodes of a dump, in document order. The `resource-id` of a view of React Native is its
 * `testID` (`start`); a view of Android has `package:id/name` (`android:id/button1`).
 * @param {string} xml
 * @returns {UiNode[]}
 */
export function parseNodes(xml) {
  /** @type {UiNode[]} */
  const nodes = [];
  for (const match of xml.matchAll(/<node\b[^>]*>/g)) {
    const tag = match[0];
    const b = /\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]/.exec(attribute(tag, "bounds"));
    if (!b) continue;
    nodes.push({
      id: attribute(tag, "resource-id"),
      text: attribute(tag, "text"),
      desc: attribute(tag, "content-desc"),
      bounds: [Number(b[1]), Number(b[2]), Number(b[3]), Number(b[4])],
    });
  }
  return nodes;
}

/**
 * The first node whose `resource-id` is `id`, as React Native gives it (`start`) or with a package
 * (`org.obop.rutsa:id/start`); `null` when there is none.
 * @param {UiNode[]} nodes
 * @param {string} id
 */
export function byId(nodes, id) {
  return nodes.find((node) => node.id === id || node.id.endsWith(`:id/${id}`)) ?? null;
}

/**
 * The first node whose text or description is `text`, whatever its case (Android may write the
 * buttons of a dialog in capitals); `null` when there is none.
 * @param {UiNode[]} nodes
 * @param {string} text
 */
export function byText(nodes, text) {
  const wanted = text.toLowerCase();
  return (
    nodes.find(
      (node) => node.text.toLowerCase() === wanted || node.desc.toLowerCase() === wanted,
    ) ?? null
  );
}

/**
 * The point a tap goes to: the middle of the node.
 * @param {UiNode} node
 */
export function center(node) {
  const [x1, y1, x2, y2] = node.bounds;
  return { x: Math.round((x1 + x2) / 2), y: Math.round((y1 + y2) / 2) };
}

/**
 * The km of a text of the screen (`4.0 km`, `1,234.6 km`); `null` when it says no km.
 * @param {string} text
 */
export function parseKm(text) {
  const match = /^([\d,]+(?:\.\d+)?) km$/.exec(text.trim());
  return match ? Number(match[1].replaceAll(",", "")) : null;
}

/**
 * Whether the km on the screen fit the reference: the screen rounds to 0.1 km, so 0.05 km either
 * way is its rounding, and `tolerance` (a share of the reference) is what the recording may lose
 * or add (a fix the thinning leaves out at a corner, a fix the emulator gives late).
 * @param {number} shownKm
 * @param {number} referenceM
 * @param {number} tolerance
 */
export function judgeKm(shownKm, referenceM, tolerance) {
  const expectedKm = referenceM / 1000;
  const allowedKm = 0.05 + expectedKm * tolerance;
  const offKm = shownKm - expectedKm;
  return { ok: Math.abs(offKm) <= allowedKm, expectedKm, offKm, allowedKm };
}
