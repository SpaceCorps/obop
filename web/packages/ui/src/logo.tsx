/**
 * The obop logo, drawn as inline SVG in one colour (`currentColor`).
 *
 * The mark is an "o" with a satellite: a ring that is open on one side and a dot that sits in the
 * opening, one platform with the business in orbit. The wordmark is lowercase, geometric and
 * monoline, drawn from the same circles so it needs no font. Rules: docs/brand/BRAND.md.
 */
export type LogoProps = {
  /** `full` is mark and wordmark, `mark` the symbol alone, `word` the wordmark alone. */
  variant?: "full" | "mark" | "word";
  /** Height in CSS pixels. The minimum is 16 for the mark and 20 for the full logo. */
  size?: number;
  className?: string;
  /** Accessible name; the logo is a graphic, so it defaults to "obop". Pass "" to hide it. */
  title?: string;
};

const MARK = (
  <g strokeWidth="4">
    <path d="M16 5a11 11 0 1 0 11 11" />
    <circle cx="23.1" cy="9.2" r="4.2" fill="currentColor" stroke="none" />
  </g>
);

const WORD = (
  <>
    <circle cx="8.4" cy="19" r="6.7" />
    <path d="M22.4 4.4V24.5" />
    <circle cx="29.1" cy="19" r="6.7" />
    <circle cx="49.8" cy="19" r="6.7" />
    <path d="M63.8 12V34" />
    <circle cx="70.5" cy="19" r="6.7" />
  </>
);

export function Logo({ variant = "full", size = 24, className, title = "obop" }: LogoProps) {
  const box = variant === "mark" ? "0 0 32 32" : variant === "word" ? "0 0 79 36" : "0 0 121 36";
  const [w, h] = box.split(" ").slice(2).map(Number) as [number, number];
  return (
    <svg
      viewBox={box}
      width={(size * w) / h}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth="3.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className ? `logo ${className}` : "logo"}
      role={title ? "img" : undefined}
      aria-label={title || undefined}
      aria-hidden={title ? undefined : true}
      focusable="false"
    >
      {variant === "mark" && MARK}
      {variant === "word" && WORD}
      {variant === "full" && (
        <>
          <g transform="translate(0 3)">{MARK}</g>
          <g transform="translate(42 0)">{WORD}</g>
        </>
      )}
    </svg>
  );
}

/** Raw SVG source of the mark, for places that cannot render React (favicons, docs). */
export const MARK_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="none" stroke="currentColor" stroke-width="4" stroke-linecap="round"><path d="M16 5a11 11 0 1 0 11 11"/><circle cx="23.1" cy="9.2" r="4.2" fill="currentColor" stroke="none"/></svg>';
