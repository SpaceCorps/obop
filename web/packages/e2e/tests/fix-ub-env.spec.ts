// Where the harness looks for the binary when OBOP_BIN is not set (docs/ISSUES.md D13): Cargo
// writes `obop.exe` on Windows, so the debug build is found there without setting OBOP_BIN.
import { expect, test } from "@playwright/test";
import { defaultBinary } from "../support/env.ts";

test.describe("the default binary of the harness", () => {
  test("is obop.exe on Windows", () => {
    expect(defaultBinary("win32")).toMatch(/[\\/]target[\\/]debug[\\/]obop\.exe$/);
  });

  test("is obop elsewhere", () => {
    expect(defaultBinary("linux")).toMatch(/[\\/]target[\\/]debug[\\/]obop$/);
    expect(defaultBinary("darwin")).toMatch(/[\\/]target[\\/]debug[\\/]obop$/);
  });
});
