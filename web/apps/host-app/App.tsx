import { BRIDGE_VERSION, USER_AGENT_TOKEN } from "@obop/host-bridge";
import { tokens } from "@obop/ui-native";
import Constants from "expo-constants";
import { StatusBar } from "expo-status-bar";
import { Platform, StyleSheet, Text, useColorScheme } from "react-native";
import { SafeAreaProvider, SafeAreaView } from "react-native-safe-area-context";
import { WebView } from "react-native-webview";
import { identity } from "./config/identity.ts";

// The skeleton of the foundation step: one bare WebView on the default server of the build. It is
// here to show that react-native-webview builds and draws on this React Native and that Metro
// resolves the two workspace packages. The address screen, the host bar, the navigation rules, the
// error screen and the bridge are the next step, which replaces this file.

/** What `app.config.ts` puts into `extra`, read without trusting its shape. */
function buildExtra(): { defaultServer: string; webviewDebug: boolean } {
  const extra: Record<string, unknown> = Constants.expoConfig?.extra ?? {};
  return {
    defaultServer: typeof extra.defaultServer === "string" ? extra.defaultServer : "",
    webviewDebug: extra.webviewDebug === true,
  };
}

export default function App() {
  const dark = useColorScheme() === "dark";
  const palette = dark ? tokens.dark : tokens.light;
  const { defaultServer, webviewDebug } = buildExtra();

  return (
    <SafeAreaProvider>
      <SafeAreaView style={[styles.fill, { backgroundColor: palette.bg }]}>
        <StatusBar style={dark ? "light" : "dark"} />
        {defaultServer === "" ? (
          <Text style={[styles.note, { color: palette.text }]}>
            This build names no server (OBOP_DEFAULT_SERVER).
          </Text>
        ) : (
          <WebView
            source={{ uri: defaultServer }}
            style={styles.fill}
            applicationNameForUserAgent={`${USER_AGENT_TOKEN}/${BRIDGE_VERSION} (${Platform.OS}; ${identity.version})`}
            webviewDebuggingEnabled={webviewDebug}
          />
        )}
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  note: { padding: 24, fontSize: 16 },
});
