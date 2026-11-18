import { InsetsProvider, Screen, Text, ThemeProvider } from "@obop/ui-native";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState, type ReactNode } from "react";
import { SafeAreaProvider, useSafeAreaInsets } from "react-native-safe-area-context";
import { t, tp } from "./src/i18n/index.ts";
import { databaseInfo, getStore } from "./src/native/store.ts";

/** Hands the space of the system bars to the components of `@obop/ui-native`. */
function Insets({ children }: { children: ReactNode }) {
  const insets = useSafeAreaInsets();
  return <InsetsProvider insets={insets}>{children}</InsetsProvider>;
}

/**
 * For now: one line that shows the database on the phone opened, encrypted, with its number of
 * trips. The screens of the prototype (block D of the plan) replace it.
 */
function DatabaseLine() {
  const [line, setLine] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    (async () => {
      const trips = await (await getStore()).listTrips();
      const { cipherVersion } = await databaseInfo();
      return `${tp("trips.count", trips.length)} · SQLCipher ${cipherVersion}`;
    })().then(
      (text) => live && setLine(text),
      () => live && setLine(t("error.database")),
    );
    return () => {
      live = false;
    };
  }, []);
  return line === null ? null : <Text tone="muted">{line}</Text>;
}

export default function App() {
  return (
    <SafeAreaProvider>
      <ThemeProvider>
        <Insets>
          <Screen title="Rutsa">
            <Text tone="muted">Mileage log</Text>
            <DatabaseLine />
          </Screen>
          <StatusBar style="auto" />
        </Insets>
      </ThemeProvider>
    </SafeAreaProvider>
  );
}
