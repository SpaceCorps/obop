import { createHostBridge } from "@obop/host-bridge";
import { Text, useInsets, useTheme } from "@obop/ui-native";
import { useEffect, useMemo, useRef, type RefObject } from "react";
import { ActivityIndicator, Platform, StyleSheet, View } from "react-native";
import { WebView } from "react-native-webview";
import { identity } from "../../config/identity.ts";
import { hostAndPort, isCleartext, startAddress } from "../core/address.ts";
import { isTestBuild } from "../core/build.ts";
import { createMessagePump } from "../core/commands.ts";
import { assignScript } from "../core/nav-policy.ts";
import { newSince, type ServerRecord } from "../core/server-record.ts";
import {
  answering,
  requestVerdict,
  webViewMounted,
  windowVerdict,
  type ShellState,
} from "../core/shell.ts";
import type { ShellStore } from "../core/store.ts";
import { webViewProps } from "../core/webview-props.ts";
import { t } from "../i18n/index.ts";
import { build } from "../native/build.ts";
import { hostPlatform, nativeHandlers } from "../native/commands.ts";
import { openExternal } from "../native/system.ts";
import { HostBar } from "./HostBar.tsx";

/**
 * The WebView as a ref holds it. The class of the library takes the type of extra props as a
 * parameter and reads `undefined` as "no props at all", so the parameter is written out.
 */
export type WebViewHandle = WebView<object>;

/** The library's own loading and error views draw nothing: the app's screens are the only ones. */
const nothing = () => <View />;

type PageProps = {
  /** The key of this WebView: every event it sends carries it. */
  mount: number;
  server: ServerRecord;
  store: ShellStore;
  /** How many reloads the app has asked for. */
  reloads: number;
  ground: string;
  webRef: RefObject<WebViewHandle | null>;
};

/**
 * One WebView on the start address of the server, locked to its origin. Every question the
 * WebView asks is answered from the reducer's state at that instant, and everything it reports
 * goes to the reducer with the number of this mount.
 */
function Page({ mount, server, store, reloads, ground, webRef }: PageProps) {
  const { dispatch, get } = store;
  const switches = useMemo(
    () =>
      webViewProps({
        build,
        start: startAddress(server),
        platform: hostPlatform,
        version: identity.version,
      }),
    [server],
  );

  // The bridge of this mount: the three commands, answered only to the instance's document.
  const pump = useMemo(() => {
    const bridge = createHostBridge({
      origin: server.origin,
      handlers: nativeHandlers({
        theme: (theme) => dispatch({ type: "theme", theme }),
        settings: () => dispatch({ type: "open_settings" }),
      }),
    });
    return createMessagePump({
      bridge,
      answering: () => answering(get(), mount),
      inject: (script) => webRef.current?.injectJavaScript(script),
    });
  }, [server.origin, mount, dispatch, get, webRef]);

  // A reload the app asked for (Retry, or iOS ended the content process). The count at the time
  // of the mount is not one: a new WebView loads by itself.
  const reloadsAtMount = useRef(reloads);
  useEffect(() => {
    if (reloads > reloadsAtMount.current) webRef.current?.reload();
  }, [reloads, webRef]);

  return (
    <WebView
      ref={webRef}
      {...switches}
      style={[styles.fill, { backgroundColor: ground }]}
      containerStyle={{ backgroundColor: ground }}
      renderLoading={nothing}
      renderError={nothing}
      onShouldStartLoadWithRequest={(request) => {
        const verdict = requestVerdict(get(), {
          mount,
          url: request.url,
          // Android does not say which frame asks; there every request counts as the top one.
          isTopFrame: Platform.OS === "ios" ? request.isTopFrame : true,
        });
        if (verdict === "allow") return true;
        if (verdict === "external") openExternal(request.url);
        if (verdict === "forwarded") dispatch({ type: "forwarded", mount, url: request.url });
        return false;
      }}
      onOpenWindow={(event) => {
        const url = event.nativeEvent.targetUrl;
        const verdict = windowVerdict(get(), { mount, url });
        if (verdict === "assign") webRef.current?.injectJavaScript(assignScript(url));
        if (verdict === "external") openExternal(url);
      }}
      onNavigationStateChange={(navigation) =>
        dispatch({ type: "nav", mount, url: navigation.url, canGoBack: navigation.canGoBack })
      }
      onLoad={(event) => dispatch({ type: "load_end", mount, url: event.nativeEvent.url })}
      onError={(event) =>
        dispatch({
          type: "load_error",
          mount,
          code: event.nativeEvent.code,
          description: event.nativeEvent.description,
        })
      }
      onHttpError={(event) =>
        dispatch({
          type: "http_error",
          mount,
          url: event.nativeEvent.url,
          status: event.nativeEvent.statusCode,
        })
      }
      onRenderProcessGone={() => dispatch({ type: "process_gone", mount })}
      onContentProcessDidTerminate={() => dispatch({ type: "content_terminated", mount })}
      onMessage={(event) => void pump(event.nativeEvent.data, event.nativeEvent.url)}
    />
  );
}

export type WebScreenProps = {
  state: ShellState & { server: ServerRecord };
  store: ShellStore;
  /** The colour behind the page and under the system bars. */
  ground: string;
  /** Another screen of the app lies over this one: it is hidden from a screen reader. */
  covered: boolean;
  webRef: RefObject<WebViewHandle | null>;
};

/**
 * The back office, full screen. The native side pads the four safe areas and paints the ground
 * behind them; the page keeps its own safe-area insets at 0. Over the top edge of the page lies
 * the host bar, and while a document of the instance is not on top, a cover.
 */
export function WebScreen({ state, store, ground, covered, webRef }: WebScreenProps) {
  const theme = useTheme();
  const insets = useInsets();
  const { server } = state;
  const host = hostAndPort(server.origin);
  // An opaque cover while a foreign document is or was on top; a spinner alone until the first
  // load of a mount has ended.
  const foreign = state.top === "foreign";
  const waiting = foreign || !state.loadedOnce;
  return (
    <View
      style={[
        styles.fill,
        {
          backgroundColor: ground,
          paddingTop: insets.top,
          paddingBottom: insets.bottom,
          paddingLeft: insets.left,
          paddingRight: insets.right,
        },
      ]}
      importantForAccessibility={covered ? "no-hide-descendants" : "auto"}
      accessibilityElementsHidden={covered}
      testID="web-screen"
    >
      {/* The wrapper sees a touch that lands on the page; that is what lets the host bar go. */}
      <View style={styles.fill} onTouchStart={() => store.dispatch({ type: "page_touched" })}>
        {webViewMounted(state) ? (
          <Page
            key={state.mount}
            mount={state.mount}
            server={server}
            store={store}
            reloads={state.reloads}
            ground={ground}
            webRef={webRef}
          />
        ) : null}
        {waiting ? (
          <View
            pointerEvents={foreign ? "auto" : "none"}
            style={[
              StyleSheet.absoluteFill,
              styles.cover,
              { gap: theme.space[3], backgroundColor: foreign ? ground : "transparent" },
            ]}
            testID="web-cover"
          >
            <ActivityIndicator color={theme.c.accentFill} size="large" />
            {foreign ? <Text tone="muted">{t("cover.opening", { host })}</Text> : null}
          </View>
        ) : null}
      </View>
      {state.bar.shown ? (
        <View
          style={[styles.bar, { top: insets.top, left: insets.left, right: insets.right }]}
          testID="host-bar-frame"
        >
          <HostBar
            host={host}
            notEncrypted={isCleartext(server.origin)}
            testBuild={isTestBuild(build)}
            newSince={newSince(server, Date.now())}
            onPress={() => store.dispatch({ type: "open_settings" })}
          />
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  cover: { alignItems: "center", justifyContent: "center" },
  bar: { position: "absolute", elevation: 4 },
});
