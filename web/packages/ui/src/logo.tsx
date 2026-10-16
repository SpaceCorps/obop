import { LOGO_BOX, MARK, MARK_STROKE, WORDMARK, WORD_STROKE, type LogoShape } from "./logo-data.ts";

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

function draw(shapes: LogoShape[]) {
  return shapes.map((s, i) =>
    s.t === "path" ? (
      <path key={i} d={s.d} />
    ) : (
      <circle
        key={i}
        cx={s.cx}
        cy={s.cy}
        r={s.r}
        {...(s.dot ? { fill: "currentColor", stroke: "none" } : {})}
      />
    ),
  );
}

export function Logo({ variant = "full", size = 24, className, title = "obop" }: LogoProps) {
  const [w, h] = LOGO_BOX[variant];
  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      width={(size * w) / h}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={WORD_STROKE}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className ? `logo ${className}` : "logo"}
      role={title ? "img" : undefined}
      aria-label={title || undefined}
      aria-hidden={title ? undefined : true}
      focusable="false"
    >
      {variant === "mark" && <g strokeWidth={MARK_STROKE}>{draw(MARK)}</g>}
      {variant === "word" && draw(WORDMARK)}
      {variant === "full" && (
        <>
          <g transform="translate(0 3)" strokeWidth={MARK_STROKE}>
            {draw(MARK)}
          </g>
          <g transform="translate(42 0)">{draw(WORDMARK)}</g>
        </>
      )}
    </svg>
  );
}
