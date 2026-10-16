import type { CSSProperties } from "react";
import { appColor, appIconName } from "./app-colors.ts";
import { hexToOklch, oklchToHex } from "./colors.ts";
import { ICONS, type IconName, type Shape } from "./icon-data.ts";

function shape(s: Shape, i: number) {
  if (s.startsWith("c:")) {
    const [cx, cy, r] = s.slice(2).split(",");
    return <circle key={i} cx={cx} cy={cy} r={r} />;
  }
  if (s.startsWith("d:")) {
    const [cx, cy, r] = s.slice(2).split(",");
    return <circle key={i} cx={cx} cy={cy} r={r} fill="currentColor" stroke="none" />;
  }
  if (s.startsWith("r:")) {
    const [x, y, w, h, rx] = s.slice(2).split(",");
    return <rect key={i} x={x} y={y} width={w} height={h} rx={rx} />;
  }
  return <path key={i} d={s} />;
}

/** Icons that point along the reading direction: mirrored when the page is right-to-left. */
const DIRECTIONAL = new Set<string>([
  "chevron-left",
  "chevron-right",
  "arrow-right",
  "log-out",
  "panel-left",
  "undo",
  "external",
]);

export type IconProps = {
  name: IconName;
  size?: number;
  /** An accessible name. Without it the icon is decorative and hidden from assistive technology. */
  label?: string;
  className?: string;
  strokeWidth?: number;
  style?: CSSProperties;
};

/** One of the obop icons: 24px grid, 1.75px stroke, rounded joins, drawn in `currentColor`. */
export function Icon({ name, size = 20, label, className, strokeWidth = 1.75, style }: IconProps) {
  const shapes = ICONS[name] ?? ICONS["app"]!;
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={["ico", DIRECTIONAL.has(name) && "flip-rtl", className].filter(Boolean).join(" ")}
      style={style}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      {shapes.map(shape)}
    </svg>
  );
}

/* ---------- App icons: a gradient tile and a white glyph ---------- */

export type AppIconProps = {
  /** Module id; picks the glyph and the default colour. */
  id?: string;
  name: string;
  /** Hex tile colour; defaults to the brand colour of the app. */
  color?: string;
  size?: number;
};

/** A rounded-square app tile: gradient from the app colour, white glyph. Decorative. */
export function AppIcon({ id = "", name, color, size = 44 }: AppIconProps) {
  const base = color && /^#[0-9a-f]{6}$/i.test(color) ? color : appColor(id, name);
  // Keep the tile dark enough for a white glyph whatever colour the admin chose.
  const lch = hexToOklch(base);
  const tile = lch.l > 0.62 ? oklchToHex({ ...lch, l: 0.62 }) : base;
  return (
    <span
      className="app-icon"
      style={
        { "--tile": tile, width: size, height: size, borderRadius: size * 0.28 } as CSSProperties
      }
      aria-hidden="true"
    >
      <Icon name={appIconName(id, name)} size={Math.round(size * 0.58)} />
    </span>
  );
}
