import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vite-plus/test";
import { InstanceLogo } from "./instance-logo.tsx";
import { Logo } from "./logo.tsx";
import { parseBrand } from "./brand.ts";

const brand = parseBrand(JSON.stringify({ name: "MOHV", slug: "mohv", v: "0123456789abcdef" }))!;
const html = (el: React.ReactElement) => renderToStaticMarkup(el);

describe("InstanceLogo with no brand", () => {
  it("is Logo, byte for byte, for both variants and for each title", () => {
    for (const variant of ["full", "mark"] as const)
      for (const title of [undefined, "obop", ""]) {
        expect(html(<InstanceLogo brand={null} variant={variant} size={30} title={title} />)).toBe(
          html(<Logo variant={variant} size={30} title={title} />),
        );
      }
  });

  it("names itself obop by default, as Logo does", () => {
    expect(html(<InstanceLogo brand={null} />)).toContain('aria-label="obop"');
  });
});

describe("InstanceLogo with a brand", () => {
  it("draws the artwork from the logo route and the name as text", () => {
    const h = html(<InstanceLogo brand={brand} variant="full" size={28} />);
    expect(h).toContain('src="/api/brand/logo.svg?v=0123456789abcdef"');
    expect(h).toContain('alt=""');
    expect(h).toContain(">MOHV</span>");
    expect(h).toContain("--instance-logo-size:28px");
    expect(h).not.toContain("<svg");
    expect(h.toLowerCase()).not.toContain("obop");
  });

  it("draws the artwork alone for the mark, with the name for a screen reader", () => {
    const h = html(<InstanceLogo brand={brand} variant="mark" size={22} />);
    expect(h).toContain('src="/api/brand/logo.svg?v=0123456789abcdef"');
    expect(h).toContain('<span class="sr-only">MOHV</span>');
    expect(h).not.toContain("instance-logo-name");
    expect(h.toLowerCase()).not.toContain("obop");
  });

  it("hides the name of a mark when asked, as Logo hides its own", () => {
    const h = html(<InstanceLogo brand={brand} variant="mark" size={22} title="" />);
    expect(h).not.toContain("MOHV");
    expect(h).toContain('alt=""');
  });

  it("shows the name of a full logo whatever the title says", () => {
    expect(html(<InstanceLogo brand={brand} title="" />)).toContain(">MOHV</span>");
    expect(html(<InstanceLogo brand={brand} title="obop" />).toLowerCase()).not.toContain("obop");
  });

  it("escapes a name that holds markup and carries no other text of the product", () => {
    const odd = { ...brand, name: '<img src=x onerror="1">&' };
    const h = html(<InstanceLogo brand={odd} />);
    expect(h).toContain("&lt;img src=x onerror=&quot;1&quot;&gt;&amp;");
    expect(h).not.toContain("<img src=x");
  });

  it("puts the version in the address of the logo, encoded", () => {
    const h = html(<InstanceLogo brand={{ ...brand, v: "" }} />);
    expect(h).toContain('src="/api/brand/logo.svg"');
  });

  it("draws the artwork from another address for a preview", () => {
    const h = html(<InstanceLogo brand={brand} src="blob:http://x/1" />);
    expect(h).toContain('src="blob:http://x/1"');
    expect(h).not.toContain("/api/brand/logo.svg");
  });

  it("passes the class on", () => {
    expect(html(<InstanceLogo brand={brand} className="signin-x" />)).toContain(
      'class="instance-logo signin-x"',
    );
    expect(html(<InstanceLogo brand={null} className="signin-x" />)).toContain("logo signin-x");
  });
});

describe("InstanceLogo reading the page", () => {
  it("is obop's mark when the tests run without a document", () => {
    expect(html(<InstanceLogo />)).toBe(html(<Logo variant="full" size={24} title="obop" />));
  });
});
