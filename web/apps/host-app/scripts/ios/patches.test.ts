// The two patches of iOS sources that the iOS build of the app needs (pnpm-workspace.yaml,
// patchedDependencies; docs/spec/mobile-host.md 12.8). Each test reads the installed source, so a
// version bump or an install that drops a patch fails here before an iOS build.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { describe, expect, test } from "vite-plus/test";

const require = createRequire(import.meta.url);
const packageDir = (name: string, from?: string) =>
  dirname(require.resolve(`${name}/package.json`, from ? { paths: [from] } : undefined));

describe("the iOS patches are installed", () => {
  test("react-native-webview: clearCache reaches the WebView on iOS (it was a no-op under the new architecture, so Change server cleared nothing)", () => {
    const source = readFileSync(
      join(packageDir("react-native-webview"), "apple", "RNCWebView.mm"),
      "utf8",
    );
    const method =
      /- \(void\)clearCache:\(BOOL\)includeDiskFiles \{([^}]*)\}/.exec(source)?.[1] ?? "";
    expect(method).toContain("[_view clearCache:includeDiskFiles];");
    // And the view's own method removes the disk cache with the rest.
    const impl = readFileSync(
      join(packageDir("react-native-webview"), "apple", "RNCWebViewImpl.m"),
      "utf8",
    );
    expect(impl).toMatch(
      /- \(void\)clearCache:\(BOOL\)includeDiskFiles[\s\S]*?WKWebsiteDataTypeDiskCache/,
    );
  });

  test("expo-modules-core: the pattern that crashes the Swift compiler of Xcode 27.2 is gone", () => {
    const core = packageDir("expo-modules-core", packageDir("expo"));
    const source = readFileSync(
      join(core, "ios", "Core", "Functions", "AsyncFunctionDefinition.swift"),
      "utf8",
    );
    expect(source).not.toContain("case .failure(let error as any Error):");
    expect(source).toContain("case .failure(let error):");
  });
});
