import { describe, expect, test } from "vite-plus/test";
import { simulatorAppPath, simulatorBuildArgs } from "./xcode-lib.mjs";

const base = {
  workspace: "/b/ios/obopMileage.xcworkspace",
  scheme: "obopMileage",
  derivedData: "/b/ios/build",
};

describe("the Simulator build", () => {
  test("is a build for the Simulator that is not signed", () => {
    const args = simulatorBuildArgs(base);
    expect(args).toContain("CODE_SIGNING_ALLOWED=NO");
    expect(args).toContain("CODE_SIGNING_REQUIRED=NO");
    expect(args.slice(args.indexOf("-sdk"), args.indexOf("-sdk") + 2)).toEqual([
      "-sdk",
      "iphonesimulator",
    ]);
    expect(args.slice(args.indexOf("-configuration"), args.indexOf("-configuration") + 2)).toEqual([
      "-configuration",
      "Release",
    ]);
    expect(args).toContain("ARCHS=arm64");
    expect(args.at(-1)).toBe("build");
    // Nothing about an account, a team or a provisioning profile.
    expect(args.join(" ")).not.toMatch(/team|provision|authenticat|apple ?id/i);
  });

  test("takes a configuration and architectures from the caller", () => {
    const args = simulatorBuildArgs({ ...base, configuration: "Debug", archs: "arm64 x86_64" });
    expect(args).toContain("Debug");
    expect(args).toContain("ARCHS=arm64 x86_64");
  });

  test("refuses what could change the command", () => {
    expect(() => simulatorBuildArgs({ ...base, configuration: "Release; rm -rf" })).toThrow(
      /Debug or Release/,
    );
    expect(() => simulatorBuildArgs({ ...base, archs: "arm64;x" })).toThrow(/architectures/);
    expect(() => simulatorBuildArgs({ ...base, scheme: "a b" })).toThrow(/scheme/);
    expect(() => simulatorBuildArgs({ ...base, workspace: "/b/ios/a$b.xcworkspace" })).toThrow(
      /workspace/,
    );
  });

  test("the app is where xcodebuild puts it", () => {
    expect(simulatorAppPath(base)).toBe(
      "/b/ios/build/Build/Products/Release-iphonesimulator/obopMileage.app",
    );
    expect(simulatorAppPath({ ...base, configuration: "Debug" })).toBe(
      "/b/ios/build/Build/Products/Debug-iphonesimulator/obopMileage.app",
    );
  });
});
