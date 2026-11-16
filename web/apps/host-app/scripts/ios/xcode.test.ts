import { describe, expect, test } from "vite-plus/test";
import { simulatorAppPath, simulatorBuildArgs } from "./xcode-lib.mjs";

const base = {
  workspace: "/b/ios/obopHost.xcworkspace",
  scheme: "obopHost",
  derivedData: "/b/dd",
};

const pair = (args: string[], flag: string) =>
  args.slice(args.indexOf(flag), args.indexOf(flag) + 2);

describe("the Simulator build", () => {
  test("is a build for the Simulator, signed ad hoc by default, with no account, team or profile", () => {
    const args = simulatorBuildArgs(base);
    expect(args).toContain("CODE_SIGN_IDENTITY=-");
    expect(args).toContain("CODE_SIGNING_ALLOWED=YES");
    expect(args).not.toContain("CODE_SIGNING_ALLOWED=NO");
    expect(args).toContain("CODE_SIGNING_REQUIRED=NO");
    expect(args).toContain("DEVELOPMENT_TEAM=");
    expect(args).toContain("PROVISIONING_PROFILE_SPECIFIER=");
    expect(pair(args, "-sdk")).toEqual(["-sdk", "iphonesimulator"]);
    expect(pair(args, "-destination")).toEqual(["-destination", "generic/platform=iOS Simulator"]);
    expect(pair(args, "-configuration")).toEqual(["-configuration", "Release"]);
    expect(pair(args, "-derivedDataPath")).toEqual(["-derivedDataPath", "/b/dd"]);
    expect(args).toContain("ARCHS=arm64");
    expect(args.at(-1)).toBe("build");
    // Nothing that signs in, picks a team or updates a profile.
    expect(args.join(" ")).not.toMatch(
      /allowProvisioning|authenticat|apple ?id|DEVELOPMENT_TEAM=\S|iphoneos|archive/i,
    );
  });

  test("unsigned on request: CODE_SIGNING_ALLOWED=NO and no identity", () => {
    const args = simulatorBuildArgs({ ...base, signing: "none" });
    expect(args).toContain("CODE_SIGNING_ALLOWED=NO");
    expect(args).not.toContain("CODE_SIGN_IDENTITY=-");
    expect(args).not.toContain("CODE_SIGNING_ALLOWED=YES");
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
    expect(() => simulatorBuildArgs({ ...base, signing: "automatic" })).toThrow(/adhoc or none/);
    expect(() => simulatorBuildArgs({ ...base, scheme: "a b" })).toThrow(/scheme/);
    expect(() => simulatorBuildArgs({ ...base, workspace: "/b/ios/a$b.xcworkspace" })).toThrow(
      /workspace/,
    );
  });

  test("the app is where xcodebuild puts it", () => {
    expect(simulatorAppPath(base)).toBe(
      "/b/dd/Build/Products/Release-iphonesimulator/obopHost.app",
    );
    expect(simulatorAppPath({ ...base, configuration: "Debug" })).toBe(
      "/b/dd/Build/Products/Debug-iphonesimulator/obopHost.app",
    );
  });
});
