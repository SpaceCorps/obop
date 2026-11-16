// The page tells the host which colour its ground has, so that the status bar, the safe areas and
// the space behind the page are drawn in it (`host.theme`).
//
// The ground is the background of the first child of `#root` (the shell, the sign-in page, a gate),
// over the one of `<body>`. Its computed value can be an `oklch()` text, which React Native cannot
// read, so the colour is painted on a canvas of one pixel and read back as red, green and blue.
// Whether it is a dark colour is worked out from its luminance, not from the name of the theme: an
// accent or a later theme can change the ground without changing `data-theme`.
import type { Scheme } from "@obop/host-bridge";

/** Red, green, blue and alpha, each 0 to 255. */
export type Rgba = readonly [number, number, number, number];
/** Reads a CSS colour as it is painted, or `null` for a text that is not a colour. */
export type ColourReader = (css: string) => Rgba | null;
/** The parameters of `host.theme`. */
export type ThemeMessage = { scheme: Scheme; background: string };

type Rgb = readonly [number, number, number];

/** `#rrggbb` in lower case. */
export function hex(rgb: Rgb): string {
  return `#${rgb.map((part) => Math.round(part).toString(16).padStart(2, "0")).join("")}`;
}

/** The relative luminance of WCAG 2: 0 for black, 1 for white. */
export function luminance([r, g, b]: Rgb): number {
  const linear = (part: number) => {
    const s = part / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/**
 * A ground is dark when white marks on it are easier to read than black ones: below a luminance of
 * 0.179 the contrast of white is the higher of the two. The host draws the status bar's clock and
 * icons by this.
 */
export function schemeOf(rgb: Rgb): Scheme {
  return luminance(rgb) < 0.179 ? "dark" : "light";
}

/** `top` painted over `under`, which is opaque. */
function over(top: Rgba, under: Rgb): Rgb {
  const a = top[3] / 255;
  return [
    top[0] * a + under[0] * (1 - a),
    top[1] * a + under[1] * (1 - a),
    top[2] * a + under[2] * (1 - a),
  ];
}

/** What the page would report now, or `null` when nothing paints a ground yet. */
export function measureTheme(doc: Document, colour: ColourReader): ThemeMessage | null {
  const view = doc.defaultView;
  if (!view) return null;
  let ground: Rgb | null = null;
  // From the back to the front: the body, then the screen that React drew over it.
  for (const element of [doc.body, doc.getElementById("root")?.firstElementChild]) {
    if (!element) continue;
    const painted = colour(view.getComputedStyle(element).backgroundColor);
    if (painted === null || painted[3] === 0) continue;
    ground =
      painted[3] === 255 || ground === null
        ? [painted[0], painted[1], painted[2]]
        : over(painted, ground);
  }
  return ground === null ? null : { scheme: schemeOf(ground), background: hex(ground) };
}

/**
 * The reader of a real page: one canvas of one pixel. A text the canvas does not take as a colour
 * leaves its fill style as it was, which two different marker colours show.
 */
export function canvasColour(doc: Document): ColourReader {
  const canvas = doc.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  let context: CanvasRenderingContext2D | null = null;
  try {
    context = canvas.getContext("2d", { willReadFrequently: true });
  } catch {
    // A page without a canvas reads no colour and reports none.
  }
  return (css) => {
    if (context === null || css === "") return null;
    context.fillStyle = "#010203";
    context.fillStyle = css;
    const first = context.fillStyle;
    context.fillStyle = "#040506";
    context.fillStyle = css;
    if (context.fillStyle !== first) return null;
    context.clearRect(0, 0, 1, 1);
    context.fillRect(0, 0, 1, 1);
    const [r = 0, g = 0, b = 0, a = 0] = context.getImageData(0, 0, 1, 1).data;
    return [r, g, b, a];
  };
}

/**
 * Reports the ground once and then whenever it changed. It looks again when an attribute of
 * `<html>` changes (the theme, the accent), when React puts another screen into `#root` and when
 * the system switches between light and dark. `send` is `host.theme`; a report that the host did
 * not take is sent again at the next look. The return value stops the reporter.
 */
export function startThemeReporter(deps: {
  doc: Document;
  colour: ColourReader;
  send: (theme: ThemeMessage) => unknown;
}): () => void {
  const { doc, colour, send } = deps;
  const view = doc.defaultView;
  if (!view) return () => {};
  let sent = "";
  let waiting = false;
  let stopped = false;

  const look = () => {
    waiting = false;
    if (stopped) return;
    const theme = measureTheme(doc, colour);
    if (theme === null) return;
    const key = `${theme.scheme} ${theme.background}`;
    if (key === sent) return;
    sent = key;
    Promise.resolve()
      .then(() => send(theme))
      .catch(() => {
        if (sent === key) sent = "";
      });
  };
  // One look per frame, however many attributes a change of theme writes.
  const soon = () => {
    if (waiting || stopped) return;
    waiting = true;
    if (typeof view.requestAnimationFrame === "function") view.requestAnimationFrame(look);
    else view.setTimeout(look, 0);
  };

  const observer = new view.MutationObserver(soon);
  observer.observe(doc.documentElement, { attributes: true });
  const root = doc.getElementById("root");
  if (root) observer.observe(root, { childList: true });
  const system =
    typeof view.matchMedia === "function" ? view.matchMedia("(prefers-color-scheme: dark)") : null;
  system?.addEventListener("change", soon);
  soon();

  return () => {
    stopped = true;
    observer.disconnect();
    system?.removeEventListener("change", soon);
  };
}
