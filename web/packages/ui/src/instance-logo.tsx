import { brandLogoUrl, pageBrand, type PageBrand } from "./brand.ts";
import { Logo } from "./logo.tsx";

/**
 * The logo of the instance the person is using (docs/spec/branding.md, sections 6 and 12).
 *
 * With no brand on the page it is `Logo`, obop's own mark, exactly as before. With a brand it is
 * the instance's artwork, an `<img>` of the logo route, and for `full` the name next to it as text;
 * `mark` is the artwork alone and carries the name as text for a screen reader. The accessible
 * name is the name of the brand either way, so no screen of an instance with a brand says `obop`.
 *
 * Apart from the Brand book, which documents obop's own mark and keeps `Logo`, it is the only
 * component of the back office and of this package that draws `Logo` (a scan in the back office
 * tests holds the other files of both to that). The control-plane app (`web/apps/platform`) is not
 * read by the scan and draws `Logo` itself, on purpose: it is the product's own sign-up, and has
 * no brand (docs/spec/branding.md, section 6).
 */
export type InstanceLogoProps = {
  /** `full` is the artwork and the name, `mark` the artwork alone. */
  variant?: "full" | "mark";
  /** Height of the artwork in CSS pixels. */
  size?: number;
  className?: string;
  /**
   * The accessible name with no brand, as for `Logo`: `obop` unless it is `""`, which hides the
   * logo from a screen reader. With a brand `""` hides the name of a `mark`; a `full` logo shows
   * its name as text whatever this says.
   */
  title?: string;
  /** The brand to draw instead of the one in the page; `null` draws obop's own mark. For tests and stories. */
  brand?: PageBrand | null;
  /** The address of the artwork instead of the logo route's: a preview of a file that is not saved yet. */
  src?: string;
};

export function InstanceLogo({
  variant = "full",
  size = 24,
  className,
  title = "obop",
  brand: given,
  src,
}: InstanceLogoProps) {
  const brand = given === undefined ? pageBrand() : given;
  if (!brand) return <Logo variant={variant} size={size} className={className} title={title} />;
  return (
    <span
      className={`instance-logo${className ? ` ${className}` : ""}`}
      data-variant={variant}
      style={{ ["--instance-logo-size" as string]: `${size}px` }}
    >
      <img
        className="instance-logo-art"
        src={src ?? brandLogoUrl(brand)}
        alt=""
        width={size}
        height={size}
        draggable={false}
      />
      {variant === "full" ? (
        <span className="instance-logo-name">{brand.name}</span>
      ) : (
        title !== "" && <span className="sr-only">{brand.name}</span>
      )}
    </span>
  );
}
