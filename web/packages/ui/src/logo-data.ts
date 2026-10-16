/**
 * Geometry of the obop logo, shared by the React component and the static renderer.
 * All coordinates are in a 32px-high design space; the wordmark is monoline circles and stems.
 */
export type LogoShape =
  | { t: "path"; d: string }
  | { t: "circle"; cx: number; cy: number; r: number; dot?: true };

/** The mark: a ring open on one side and a dot sitting in the opening. 32 x 32. */
export const MARK: LogoShape[] = [
  { t: "path", d: "M16 5a11 11 0 1 0 11 11" },
  { t: "circle", cx: 23.1, cy: 9.2, r: 4.2, dot: true },
];
export const MARK_STROKE = 4;

/** The wordmark "obop". 79 x 36, stroke 3.4. */
export const WORDMARK: LogoShape[] = [
  { t: "circle", cx: 8.4, cy: 19, r: 6.7 },
  { t: "path", d: "M22.4 4.4V24.5" },
  { t: "circle", cx: 29.1, cy: 19, r: 6.7 },
  { t: "circle", cx: 49.8, cy: 19, r: 6.7 },
  { t: "path", d: "M63.8 12V34" },
  { t: "circle", cx: 70.5, cy: 19, r: 6.7 },
];
export const WORD_STROKE = 3.4;

export const LOGO_BOX = {
  mark: [32, 32],
  word: [79, 36],
  full: [121, 36],
} as const;
export type LogoVariant = keyof typeof LOGO_BOX;
