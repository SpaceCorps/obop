import { describe, expect, it } from "vite-plus/test";
import { seamedName } from "./brandName.ts";

/** A soft hyphen (U+00AD), written so that it can be seen. */
const SHY = String.fromCharCode(0xad);

describe("an app name in the top bar of a phone", () => {
  it("breaks a long compound at its seams, with a soft hyphen", () => {
    expect(seamedName("Rechnungsstellung")).toBe(`Rechnungs${SHY}stellung`);
    expect(seamedName("Sähköpostimarkkinointi")).toBe(`Sähkö${SHY}posti${SHY}markkinointi`);
    // Read as text, the name is the same.
    expect(seamedName("E-postmarknadsföring").replaceAll(SHY, "")).toBe("E-postmarknadsföring");
  });

  it("leaves every other name as it is", () => {
    expect(seamedName("Invoicing")).toBe("Invoicing");
    expect(seamedName("Human Resources")).toBe("Human Resources");
  });
});
