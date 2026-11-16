// Every switch of the WebView as one plain object, so that `vp test` holds each of them. The
// screen spreads it into the component and adds only the handlers and the style.
//
// A switch goes only to the system that has it. On Android the library hands every prop it does
// not know to the native view as it is, and one of the iOS switches (`dataDetectorTypes`) then
// ends the app: the native side expects a list and gets a text.
import type { Platform } from "@obop/host-bridge";
import type { Build } from "./build.ts";
import { userAgentSuffix } from "./user-agent.ts";

/** The switches both systems have. */
export interface SharedSwitches {
  readonly source: { readonly uri: string };
  readonly applicationNameForUserAgent: string;
  readonly originWhitelist: string[];
  readonly javaScriptEnabled: true;
  readonly cacheEnabled: true;
  readonly incognito: false;
  readonly allowFileAccessFromFileURLs: false;
  readonly allowUniversalAccessFromFileURLs: false;
  readonly javaScriptCanOpenWindowsAutomatically: false;
  readonly startInLoadingState: false;
  readonly webviewDebuggingEnabled: boolean;
}

export interface AndroidSwitches {
  readonly domStorageEnabled: true;
  readonly thirdPartyCookiesEnabled: false;
  readonly allowFileAccess: false;
  readonly geolocationEnabled: false;
  readonly mixedContentMode: "never";
  readonly setSupportMultipleWindows: true;
  readonly overScrollMode: "never";
  readonly setBuiltInZoomControls: true;
  readonly setDisplayZoomControls: false;
  readonly forceDarkOn: false;
}

export interface IosSwitches {
  readonly sharedCookiesEnabled: true;
  readonly mediaCapturePermissionGrantType: "deny";
  readonly pullToRefreshEnabled: false;
  readonly bounces: false;
  readonly allowsBackForwardNavigationGestures: true;
  readonly allowsLinkPreview: false;
  readonly dataDetectorTypes: "none";
  readonly contentInsetAdjustmentBehavior: "never";
  readonly automaticallyAdjustContentInsets: false;
}

export type WebViewSwitches = SharedSwitches & Partial<AndroidSwitches> & Partial<IosSwitches>;

export interface WebViewInput {
  readonly build: Build;
  /** The address the WebView starts on: the stored origin and path. */
  readonly start: string;
  readonly platform: Platform;
  /** The version of the app, for the user agent. */
  readonly version: string;
}

/**
 * Android only.
 *
 * - The page keeps its session in `sessionStorage` and its settings in `localStorage`
 *   (`domStorageEnabled`); no third-party cookies.
 * - The page needs no file access and gets no location; mixed content is never loaded.
 * - `setSupportMultipleWindows` stays on with a handler for new windows: the library then hands
 *   the address to the app and opens nothing.
 * - No overscroll: a pull fights the lists and the board. (Android's WebView has no pull to
 *   refresh of its own.)
 * - Pinch zoom stays (an accessibility need) without the zoom buttons; the text size is not set.
 * - `forceDarkOn` off: the page has its own dark theme and follows `prefers-color-scheme`.
 */
const ANDROID: AndroidSwitches = {
  domStorageEnabled: true,
  thirdPartyCookiesEnabled: false,
  allowFileAccess: false,
  geolocationEnabled: false,
  mixedContentMode: "never",
  setSupportMultipleWindows: true,
  overScrollMode: "never",
  setBuiltInZoomControls: true,
  setDisplayZoomControls: false,
  forceDarkOn: false,
};

/**
 * iOS only.
 *
 * - `mediaCapturePermissionGrantType: "deny"`: the library's default is to prompt, so without this
 *   any script of the page could make iOS ask for the camera or the microphone.
 * - No pull to refresh and no bounce: a reload drops an unsaved form.
 * - The edge swipe goes back (an iPhone has no Back button), no link preview (it could load
 *   another origin), no data detectors.
 * - The native side pads the safe areas itself, so the WebView adjusts nothing.
 */
const IOS: IosSwitches = {
  sharedCookiesEnabled: true,
  mediaCapturePermissionGrantType: "deny",
  pullToRefreshEnabled: false,
  bounces: false,
  allowsBackForwardNavigationGestures: true,
  allowsLinkPreview: false,
  dataDetectorTypes: "none",
  contentInsetAdjustmentBehavior: "never",
  automaticallyAdjustContentInsets: false,
};

/**
 * The switches of the WebView for one system. The shared ones:
 *
 * - `originWhitelist` is `*` because the app decides every navigation itself (`nav-policy.ts`);
 *   the library's own list is a prefix match.
 * - The cache is on and there is no private mode: the page's storage stays in the app's data.
 * - No access from file addresses, and the page opens no window by itself.
 * - `startInLoadingState` off: the library's own loading view is never shown; the app's screens
 *   are the only ones.
 * - `webviewDebuggingEnabled` follows OBOP_WEBVIEW_DEBUG alone: "speaks plain http" and "open to
 *   DevTools" are separate facts with separate switches.
 */
export function webViewProps(input: WebViewInput): WebViewSwitches {
  const shared: SharedSwitches = {
    source: { uri: input.start },
    applicationNameForUserAgent: userAgentSuffix(input.platform, input.version),
    originWhitelist: ["*"],
    javaScriptEnabled: true,
    cacheEnabled: true,
    incognito: false,
    allowFileAccessFromFileURLs: false,
    allowUniversalAccessFromFileURLs: false,
    javaScriptCanOpenWindowsAutomatically: false,
    startInLoadingState: false,
    webviewDebuggingEnabled: input.build.webviewDebug,
  };
  return input.platform === "ios" ? { ...shared, ...IOS } : { ...shared, ...ANDROID };
}
