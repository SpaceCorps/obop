import { InsetsProvider, ThemeProvider, WordsProvider, type Words } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import * as SystemUI from "expo-system-ui";
import { useEffect, useMemo, useRef, useSyncExternalStore, type ReactNode } from "react";
import {
  AppState,
  BackHandler,
  KeyboardAvoidingView,
  Platform,
  StyleSheet,
  useColorScheme,
  View,
} from "react-native";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { identity } from "./config/identity.ts";
import { hostAndPort, startAddress } from "./src/core/address.ts";
import { backAction } from "./src/core/back.ts";
import { probeVerdict } from "./src/core/probe.ts";
import { encodeServerRecord, makeServerRecord } from "./src/core/server-record.ts";
import { BAR_MIN_MS, SETTLE_MS, WATCHDOG_MS } from "./src/core/shell.ts";
import { createShellStore } from "./src/core/store.ts";
import { barStyle, groundOf, schemeOfSystem, startGround } from "./src/core/theme.ts";
import { t } from "./src/i18n/index.ts";
import { build } from "./src/native/build.ts";
import { probe } from "./src/native/probe.ts";
import { serverStore } from "./src/native/storage.ts";
import { confirmChangeServer, openExternal } from "./src/native/system.ts";
import { AddressScreen } from "./src/ui/AddressScreen.tsx";
import { ErrorScreen } from "./src/ui/ErrorScreen.tsx";
import { SettingsSheet } from "./src/ui/SettingsSheet.tsx";
import { WebScreen, type WebViewHandle } from "./src/ui/WebScreen.tsx";

/** The few words the components of `@obop/ui-native` say themselves. */
const WORDS: Words = {
  back: t("common.back"),
  error: t("words.error"),
  info: t("words.info"),
  success: t("words.success"),
  warning: t("words.warning"),
  danger: t("words.danger"),
};

function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

/**
 * The app: the reducer of `src/core/shell.ts` and the things only a phone can do for it. The
 * reducer says what is on the screen; this component reads the stored address, runs the probe,
 * keeps the three timers, listens to Back and to the return to the foreground, and hands
 * addresses to the system.
 */
function Shell() {
  const store = useMemo(() => createShellStore(build), []);
  const state = useSyncExternalStore(store.subscribe, store.get);
  const { dispatch, get } = store;
  const webRef = useRef<WebViewHandle | null>(null);
  const system = schemeOfSystem(useColorScheme());

  // The stored address, once. A store that cannot be read is an app without an address.
  useEffect(() => {
    let alive = true;
    serverStore
      .read()
      .catch(() => null)
      .then((stored) => {
        if (alive) dispatch({ type: "booted", stored });
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [dispatch]);

  // The check of a typed address. The address is stored only when its own probe accepted it, and
  // the page opens only after the phone stored it.
  const probeId = state.probe?.id ?? null;
  useEffect(() => {
    const current = get().probe;
    if (probeId === null || current === null || current.id !== probeId) return;
    const { id, address } = current;
    void (async () => {
      const verdict = probeVerdict(await probe(address.origin), address, build.cleartext);
      if (verdict.kind !== "accepted") {
        dispatch({ type: "probe_done", probe: id, verdict });
        return;
      }
      const record = makeServerRecord(address, verdict.coreVersion, Date.now());
      try {
        await serverStore.write(encodeServerRecord(record));
      } catch {
        dispatch({ type: "store_failed", probe: id });
        return;
      }
      dispatch({ type: "connected", probe: id, record });
    })();
  }, [probeId, dispatch, get]);

  // The watchdog of a load the app started itself.
  const watchdog = state.watchdog;
  useEffect(() => {
    if (watchdog === null) return undefined;
    const timer = setTimeout(
      () => dispatch({ type: "watchdog_fired", ticket: watchdog }),
      WATCHDOG_MS,
    );
    return () => clearTimeout(timer);
  }, [watchdog, dispatch]);

  // After Retry a load end has to stand for a moment before the error screen goes.
  const settle = state.settle;
  useEffect(() => {
    if (settle === null) return undefined;
    const timer = setTimeout(() => dispatch({ type: "settled", ticket: settle }), SETTLE_MS);
    return () => clearTimeout(timer);
  }, [settle, dispatch]);

  // The host bar stays at least two seconds.
  const barShown = state.bar.shown;
  const barTimer = state.bar.timer;
  useEffect(() => {
    if (!barShown) return undefined;
    const timer = setTimeout(
      () => dispatch({ type: "bar_min_elapsed", ticket: barTimer }),
      BAR_MIN_MS,
    );
    return () => clearTimeout(timer);
  }, [barShown, barTimer, dispatch]);

  // A foreign address that got on top goes to the system browser, once.
  const externalId = state.external?.id ?? null;
  useEffect(() => {
    const external = get().external;
    if (externalId !== null && external !== null && external.id === externalId) {
      openExternal(external.url);
    }
  }, [externalId, get]);

  // Every return to the app shows the host bar again.
  useEffect(() => {
    let last = AppState.currentState;
    const subscription = AppState.addEventListener("change", (next) => {
      if (next === "active" && last !== "active") dispatch({ type: "foreground" });
      last = next;
    });
    return () => subscription.remove();
  }, [dispatch]);

  // Android Back: close "This app", else walk the page's history, else leave the app.
  useEffect(() => {
    const subscription = BackHandler.addEventListener("hardwareBackPress", () => {
      const now = get();
      if (now.screen !== "web" && now.screen !== "error") return false;
      const action = backAction({
        settingsOpen: now.settingsOpen,
        errorShown: now.screen === "error",
        canGoBack: now.canGoBack,
      });
      if (action === "close_settings") {
        dispatch({ type: "close_settings" });
        return true;
      }
      if (action === "web_back") {
        webRef.current?.goBack();
        return true;
      }
      return false;
    });
    return () => subscription.remove();
  }, [dispatch, get]);

  // The ground: what the page reported, else the token of the system's scheme. The same colour is
  // the window's, so that a rotation or the keyboard shows no other one.
  const onWeb = (state.screen === "web" || state.screen === "error") && state.server !== null;
  const ground = onWeb ? groundOf(state.theme, system) : startGround(system);
  useEffect(() => {
    SystemUI.setBackgroundColorAsync(ground).catch(() => undefined);
  }, [ground]);

  const changeServer = async () => {
    const server = get().server;
    if (server === null) return;
    if (!(await confirmChangeServer(hostAndPort(server.origin)))) return;
    // If the phone cannot remove the item, the old address is back at the next start; the app
    // still asks for a new one now.
    await serverStore.clear().catch(() => undefined);
    dispatch({ type: "change_server" });
  };

  if (state.screen === "boot") {
    return <View style={[styles.fill, { backgroundColor: ground }]} />;
  }
  if (state.server === null || state.screen === "address" || state.screen === "probing") {
    return (
      <>
        <StatusBar style="auto" />
        <AddressScreen
          build={build}
          value={state.field}
          note={state.note}
          busy={state.screen === "probing"}
          onChange={(text) => dispatch({ type: "field_changed", text })}
          onConnect={() => dispatch({ type: "connect" })}
          onUseForwarded={() => dispatch({ type: "use_forwarded" })}
        />
      </>
    );
  }
  const server = state.server;
  const overlaid = state.screen === "error" || state.settingsOpen;
  return (
    <View style={styles.fill}>
      {/* Over the page the status bar follows the page's ground; over a native screen, the system. */}
      <StatusBar style={overlaid ? "auto" : barStyle(state.theme?.scheme ?? system)} />
      <WebScreen
        state={{ ...state, server }}
        store={store}
        ground={ground}
        covered={overlaid}
        webRef={webRef}
      />
      {state.screen === "error" && state.problem !== null ? (
        <ErrorScreen
          origin={server.origin}
          problem={state.problem}
          retrying={state.retrying}
          onRetry={() => dispatch({ type: "retry" })}
          onChangeServer={() => void changeServer()}
          onThisApp={() => dispatch({ type: "open_settings" })}
        />
      ) : null}
      {state.settingsOpen ? (
        <SettingsSheet
          server={server}
          build={build}
          appVersion={`${identity.version} (${identity.versionCode})`}
          onClose={() => dispatch({ type: "close_settings" })}
          onOpenInBrowser={() => openExternal(startAddress(server))}
          onChangeServer={() => void changeServer()}
        />
      ) : null}
    </View>
  );
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <WordsProvider words={WORDS}>
          <Insets>
            {/* Android draws the app edge to edge and does not resize the window for the keyboard,
                so the app makes room itself: the Connect button and the page stay above it. On iOS
                the native screens pad themselves and the WebView scrolls its own field into view. */}
            <KeyboardAvoidingView
              style={styles.fill}
              behavior="padding"
              enabled={Platform.OS === "android"}
            >
              <Shell />
            </KeyboardAvoidingView>
          </Insets>
        </WordsProvider>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
});
