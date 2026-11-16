// What kind of build this is: the three values `app.config.ts` writes into `extra` of the Expo
// config, read without trusting their shape.

export interface Build {
  /** Made with OBOP_ALLOW_CLEARTEXT=1: the build may speak plain http to local addresses. */
  readonly cleartext: boolean;
  /** Made with OBOP_WEBVIEW_DEBUG=1: the WebView is open to DevTools. */
  readonly webviewDebug: boolean;
  /** OBOP_DEFAULT_SERVER: what the address field starts with, or an empty text. */
  readonly defaultServer: string;
}

/**
 * The flags of the build. Only the value `true` switches something on, and a default server is
 * taken only when it is text, so a config that lacks a key, or holds `"true"` or `1`, gives the
 * plain build.
 */
export function readBuild(extra: unknown): Build {
  const values: Record<string, unknown> =
    typeof extra === "object" && extra !== null ? (extra as Record<string, unknown>) : {};
  return {
    cleartext: values.cleartextAllowed === true,
    webviewDebug: values.webviewDebug === true,
    defaultServer: typeof values.defaultServer === "string" ? values.defaultServer.trim() : "",
  };
}

/**
 * A test build is one that can speak plain http or is open to DevTools. The host bar and "This
 * app" say so, and no test build may be signed in to an instance with real data.
 */
export function isTestBuild(build: Build): boolean {
  return build.cleartext || build.webviewDebug;
}
