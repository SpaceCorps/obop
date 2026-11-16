import { hostBridgeVersion } from "@obop/host-bridge";
import { describe, expect, test } from "vite-plus/test";
import type { Build } from "./build.ts";
import { userAgentSuffix } from "./user-agent.ts";
import { webViewProps } from "./webview-props.ts";

const PLAIN: Build = { cleartext: false, webviewDebug: false, defaultServer: "" };
const START = "https://erp.example.com/admin/";

const android = (build: Build = PLAIN) =>
  webViewProps({ build, start: START, platform: "android", version: "0.1.0" });
const ios = (build: Build = PLAIN) =>
  webViewProps({ build, start: START, platform: "ios", version: "0.1.0" });

/** The switches only iOS has. One of them, handed to the Android view, ends the app. */
const IOS_ONLY = [
  "sharedCookiesEnabled",
  "mediaCapturePermissionGrantType",
  "pullToRefreshEnabled",
  "bounces",
  "allowsBackForwardNavigationGestures",
  "allowsLinkPreview",
  "dataDetectorTypes",
  "contentInsetAdjustmentBehavior",
  "automaticallyAdjustContentInsets",
];
const ANDROID_ONLY = [
  "domStorageEnabled",
  "thirdPartyCookiesEnabled",
  "allowFileAccess",
  "geolocationEnabled",
  "mixedContentMode",
  "setSupportMultipleWindows",
  "overScrollMode",
  "setBuiltInZoomControls",
  "setDisplayZoomControls",
  "forceDarkOn",
];

describe("the switches of the WebView", () => {
  test("it starts on the stored address, and the app decides every navigation itself", () => {
    for (const props of [android(), ios()]) {
      expect(props.source).toEqual({ uri: START });
      // The library's own list is a prefix match; `nav-policy.ts` is the rule.
      expect(props.originWhitelist).toEqual(["*"]);
      expect(props.javaScriptEnabled).toBe(true);
    }
  });

  test("every file switch is off", () => {
    for (const props of [android(), ios()]) {
      expect(props.allowFileAccessFromFileURLs).toBe(false);
      expect(props.allowUniversalAccessFromFileURLs).toBe(false);
    }
    expect(android().allowFileAccess).toBe(false);
  });

  test("the page opens no window by itself, and a new window is handed to the app", () => {
    for (const props of [android(), ios()]) {
      expect(props.javaScriptCanOpenWindowsAutomatically).toBe(false);
    }
    // With the switch on and a handler, the Android library opens nothing and reports the address.
    expect(android().setSupportMultipleWindows).toBe(true);
  });

  test("iOS never asks for the camera or the microphone on a script's word", () => {
    expect(ios().mediaCapturePermissionGrantType).toBe("deny");
  });

  test("Android gets no location, no mixed content and no third-party cookies", () => {
    const props = android();
    expect(props.geolocationEnabled).toBe(false);
    expect(props.mixedContentMode).toBe("never");
    expect(props.thirdPartyCookiesEnabled).toBe(false);
    expect(props.forceDarkOn).toBe(false);
  });

  test("the page keeps its storage: a session survives a reload, nothing is private mode", () => {
    expect(android().domStorageEnabled).toBe(true);
    for (const props of [android(), ios()]) {
      expect(props.incognito).toBe(false);
      expect(props.cacheEnabled).toBe(true);
    }
  });

  test("no pull to refresh and no overscroll: a reload drops an unsaved form", () => {
    expect(ios().pullToRefreshEnabled).toBe(false);
    expect(ios().bounces).toBe(false);
    expect(android().overScrollMode).toBe("never");
  });

  test("pinch zoom stays, without the zoom buttons", () => {
    expect(android().setBuiltInZoomControls).toBe(true);
    expect(android().setDisplayZoomControls).toBe(false);
  });

  test("the library's own loading view is never shown", () => {
    for (const props of [android(), ios()]) expect(props.startInLoadingState).toBe(false);
  });

  test("debugging follows its own flag, not the cleartext one", () => {
    for (const make of [android, ios]) {
      expect(make(PLAIN).webviewDebuggingEnabled).toBe(false);
      expect(make({ ...PLAIN, cleartext: true }).webviewDebuggingEnabled).toBe(false);
      expect(make({ ...PLAIN, webviewDebug: true }).webviewDebuggingEnabled).toBe(true);
      expect(
        make({ cleartext: true, webviewDebug: true, defaultServer: "" }).webviewDebuggingEnabled,
      ).toBe(true);
    }
  });

  test("each system gets only its own switches", () => {
    const onAndroid = Object.keys(android());
    const onIos = Object.keys(ios());
    for (const name of IOS_ONLY) {
      expect(onAndroid, name).not.toContain(name);
      expect(onIos, name).toContain(name);
    }
    for (const name of ANDROID_ONLY) {
      expect(onIos, name).not.toContain(name);
      expect(onAndroid, name).toContain(name);
    }
    // Nothing else: a new switch has to be put on one of the lists of this test.
    const shared = onAndroid.filter((name) => !ANDROID_ONLY.includes(name));
    expect(shared.sort()).toEqual(onIos.filter((name) => !IOS_ONLY.includes(name)).sort());
    expect(shared).toHaveLength(11);
  });
});

describe("the user agent", () => {
  test("the token, the major version of the bridge, the system and the app's version", () => {
    expect(userAgentSuffix("android", "0.1.0")).toBe("ObopHost/1 (android; 0.1.0)");
    expect(userAgentSuffix("ios", "1.2.3")).toBe("ObopHost/1 (ios; 1.2.3)");
    expect(android().applicationNameForUserAgent).toBe("ObopHost/1 (android; 0.1.0)");
    expect(ios().applicationNameForUserAgent).toBe("ObopHost/1 (ios; 0.1.0)");
  });

  test("the page reads the bridge version out of it", () => {
    const agent = `Mozilla/5.0 (Linux; Android 16; wv) AppleWebKit/537.36 Chrome/133.0 Mobile Safari/537.36 ${userAgentSuffix("android", "0.1.0")}`;
    expect(hostBridgeVersion(agent)).toBe(1);
  });
});
