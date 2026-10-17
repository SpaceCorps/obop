import { useMemo } from "react";
import { encode } from "uqr";

/**
 * A QR code drawn as one SVG path from the module matrix of `uqr` (MIT, no dependencies, about
 * 27 KB unminified), so the text never leaves the browser: no image service, no canvas. The
 * symbol is always dark on white with a four-module quiet zone, in every theme, because scanners
 * need that contrast. `label` is the accessible name; give the same text to a person as a typed
 * alternative next to it (the shared secret), since a picture is not enough for everyone.
 */
export function QrCode({
  value,
  label,
  size = 192,
}: {
  value: string;
  label: string;
  /** Rendered edge length in CSS pixels. */
  size?: number;
}) {
  const { path, edge } = useMemo(() => {
    const qr = encode(value, { ecc: "M", border: 0 });
    let d = "";
    for (let y = 0; y < qr.size; y++) {
      let x = 0;
      while (x < qr.size) {
        if (!qr.data[y]![x]) {
          x++;
          continue;
        }
        const start = x;
        while (x < qr.size && qr.data[y]![x]) x++;
        d += `M${start + 4} ${y + 4}h${x - start}v1h${start - x}z`;
      }
    }
    return { path: d, edge: qr.size + 8 };
  }, [value]);
  return (
    <svg
      className="qr"
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`0 0 ${edge} ${edge}`}
      shapeRendering="crispEdges"
    >
      <rect width={edge} height={edge} fill="#fff" />
      <path d={path} fill="#000" />
    </svg>
  );
}
