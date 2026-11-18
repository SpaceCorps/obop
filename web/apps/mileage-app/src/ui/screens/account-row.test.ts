import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";

// The Account screen has no renderer in this app; its row under "This phone" is read from the
// source (L28): the device's name is the title, the platform and the app version stand under it.
const source = readFileSync(fileURLToPath(new URL("./Account.tsx", import.meta.url)), "utf8");

describe("the row of this phone on the Account screen (L28)", () => {
  test("is titled with the device name, not the platform", () => {
    expect(source).toMatch(
      /<Row\s+title=\{device\.name\}\s+subtitle=\{`\$\{device\.platform\} \$\{device\.appVersion\}`\}/,
    );
    expect(source).not.toContain("title={device.platform}");
  });
});
